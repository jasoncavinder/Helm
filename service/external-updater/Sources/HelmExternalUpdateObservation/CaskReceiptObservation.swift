import Darwin
import Foundation

/// Fixed local receipt/configuration reads, never Ruby evaluation, current
/// environment configuration, or a search through historical cask definitions.
enum NativeCaskReceiptObserver {
    struct Snapshot: Equatable {
        var identities: [String: FileIdentity] = [:]
        var bytes: [String: Data] = [:]
        var claims: [String] = []
        var coverageGap: NativeCaskCoverageGap.Reason? = .missingReceipt
        var byteCount: Int { bytes.values.reduce(0) { $0 + $1.count } }
    }

    static func snapshot(token: URL, target: URL, remainingBytes: Int) throws -> Snapshot {
        let metadata = token.appendingPathComponent(".metadata", isDirectory: true)
        let descriptor = open(metadata.path, O_RDONLY | O_CLOEXEC | O_NOFOLLOW_ANY | O_DIRECTORY | O_NONBLOCK)
        guard descriptor >= 0 else {
            if errno == ENOENT { return Snapshot(coverageGap: .missingMetadata) }
            throw ObservationFailure.unreadableManagerEvidence
        }
        defer { close(descriptor) }
        let before = try FileIdentity.read(descriptor: descriptor)
        var result = Snapshot(identities: [metadata.path: before])
        for (name, limit) in [("INSTALL_RECEIPT.json", 256 * 1024), ("config.json", 64 * 1024)] {
            let path = metadata.appendingPathComponent(name, isDirectory: false)
            if let (identity, bytes) = try read(path, limit: min(limit, remainingBytes - result.byteCount)) {
                result.identities[path.path] = identity
                result.bytes[name] = bytes
            }
        }
        if let receipt = result.bytes["INSTALL_RECEIPT.json"] {
            let artifacts = try inspect(receipt: receipt, config: result.bytes["config.json"], target: target.path)
            result.claims = artifacts.claims
            result.coverageGap = artifacts.coverageGap
        }
        guard before == (try FileIdentity.read(descriptor: descriptor)),
              before == (try FileIdentity.read(metadata)) else { throw ObservationFailure.changedDuringObservation }
        return result
    }

    private static func read(_ path: URL, limit: Int) throws -> (FileIdentity, Data)? {
        let descriptor = open(path.path, O_RDONLY | O_CLOEXEC | O_NOFOLLOW_ANY | O_NONBLOCK)
        guard descriptor >= 0 else {
            if errno == ENOENT { return nil }
            throw ObservationFailure.unreadableManagerEvidence
        }
        defer { close(descriptor) }
        let before = try FileIdentity.read(descriptor: descriptor)
        guard before.isRegular, before.size > 0 else { throw ObservationFailure.unreadableManagerEvidence }
        guard before.size <= limit else { throw ObservationFailure.limitExceeded }
        var bytes = [UInt8](repeating: 0, count: Int(before.size))
        var offset = 0
        while offset < bytes.count {
            let bytesRead = bytes.withUnsafeMutableBytes {
                Darwin.read(descriptor, $0.baseAddress!.advanced(by: offset), $0.count - offset)
            }
            if bytesRead < 0, errno == EINTR { continue }
            guard bytesRead > 0 else { throw ObservationFailure.unreadableManagerEvidence }
            offset += bytesRead
        }
        guard before == (try FileIdentity.read(descriptor: descriptor)),
              before == (try FileIdentity.read(path)) else { throw ObservationFailure.changedDuringObservation }
        return (before, Data(bytes))
    }

    static func claims(receipt: Data, config: Data?, target: String) throws -> [String] {
        try inspect(receipt: receipt, config: config, target: target).claims
    }

    struct Artifacts {
        let claims: [String]
        let coverageGap: NativeCaskCoverageGap.Reason?
    }

    static func inspect(receipt: Data, config: Data?, target: String) throws -> Artifacts {
        guard receipt.count <= 256 * 1024, (config?.count ?? 0) <= 64 * 1024 else { throw ObservationFailure.limitExceeded }
        let object = try dictionary(receipt)
        // Legacy receipts without artifact declarations remain unresolved. This
        // observer never supplies a successful complete-ownership assertion.
        guard let raw = object["uninstall_artifacts"], !(raw is NSNull) else {
            return Artifacts(claims: [], coverageGap: .missingArtifactDeclarations)
        }
        guard let artifacts = raw as? [[String: Any]] else { throw ObservationFailure.unreadableManagerEvidence }
        guard artifacts.count <= 512 else { throw ObservationFailure.limitExceeded }
        var destinations = Set<String>()
        var gap: NativeCaskCoverageGap.Reason? = artifacts.isEmpty ? .emptyArtifactDeclarations : nil
        var savedDirectory: URL?
        var inspectedPaths = 0
        for artifact in artifacts {
            guard artifact.count == 1 else { throw ObservationFailure.unreadableManagerEvidence }
            guard let (kind, value) = artifact.first else { throw ObservationFailure.unreadableManagerEvidence }
            let inspected = try NativeCaskArtifactPaths.inspect(kind: kind, value: value) {
                if savedDirectory == nil { savedDirectory = try appDirectory(config) }
                guard let directory = savedDirectory else { throw ObservationFailure.unreadableManagerEvidence }
                return directory
            }
            inspectedPaths += inspected.paths.count
            guard inspectedPaths <= 4096 else { throw ObservationFailure.limitExceeded }
            if inspected.incomplete { gap = .uninspectedArtifacts }
            destinations.formUnion(inspected.paths.filter { NativeCaskArtifactPaths.overlaps($0, target: target) })
        }
        return Artifacts(claims: destinations.sorted(), coverageGap: gap)
    }

    private static func appDirectory(_ data: Data?) throws -> URL {
        guard let data else { throw ObservationFailure.unreadableManagerEvidence }
        let object = try dictionary(data)
        var result = "/Applications"
        // Homebrew's saved precedence, not current HOMEBREW_CASK_OPTS.
        for name in ["default", "env", "explicit"] {
            guard let layer = object[name] as? [String: Any] else { throw ObservationFailure.unreadableManagerEvidence }
            if let raw = layer["appdir"] {
                guard let value = raw as? String else { throw ObservationFailure.unreadableManagerEvidence }
                try validateText(value)
                guard value.hasPrefix("/") else { throw ObservationFailure.unreadableManagerEvidence }
                result = value
            }
        }
        return NativeManagerObserver.lexicalReferenceURL(result, relativeTo: URL(fileURLWithPath: "/", isDirectory: true))
    }

    private static func dictionary(_ data: Data) throws -> [String: Any] {
        guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw ObservationFailure.unreadableManagerEvidence
        }
        return object
    }

    private static func validateText(_ value: String) throws {
        guard !value.isEmpty, value.utf8.count <= 4096,
              !value.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) }) else {
            throw ObservationFailure.unreadableManagerEvidence
        }
    }
}
