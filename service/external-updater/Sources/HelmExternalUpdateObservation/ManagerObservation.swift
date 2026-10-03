import Darwin
import Foundation

/// Exclusion evidence only. Neither a missing marker nor a completed scan
/// establishes standalone provenance. No caller can deserialize trusted facts.
public struct NativeManagerEvidence: Encodable {
    public enum Disposition: String, Encodable {
        case otherManager, unresolved
    }

    public enum Exclusion: String, Encodable {
        case appStoreReceipt, homebrewCaskReference, setappLocation
    }

    public let exclusions: [Exclusion]
    public let homebrewReferences: [String]
    public let inspectedCaskEntries: Int
    public var disposition: Disposition { exclusions.isEmpty ? .unresolved : .otherManager }

    enum CodingKeys: String, CodingKey {
        case disposition, exclusions, homebrewReferences, inspectedCaskEntries
    }

    public func encode(to encoder: Encoder) throws {
        var values = encoder.container(keyedBy: CodingKeys.self)
        try values.encode(disposition, forKey: .disposition)
        try values.encode(exclusions, forKey: .exclusions)
        try values.encode(homebrewReferences, forKey: .homebrewReferences)
        try values.encode(inspectedCaskEntries, forKey: .inspectedCaskEntries)
    }
}

/// Reads Homebrew's moved-app references without executing brew, evaluating
/// cask Ruby/JSON, following their destinations or consulting Helm's inventory.
struct NativeManagerObserver {
    static let defaultCaskrooms = ["/opt/homebrew/Caskroom", "/usr/local/Caskroom"].map { URL(fileURLWithPath: $0) }
    let caskrooms: [URL]
    let entryLimit: Int

    init(caskrooms: [URL] = Self.defaultCaskrooms, entryLimit: Int = 10_000) {
        self.caskrooms = caskrooms
        self.entryLimit = entryLimit
    }

    struct Snapshot: Equatable {
        var identities: [String: FileIdentity] = [:]
        var absentRoots: [String] = []
        var links: [String: String] = [:]
        var references: [String] = []
        var entryCount = 0

        func evidence(target: URL, applicationRoots: [URL], hasStoreReceipt: Bool) -> NativeManagerEvidence {
            var exclusions: [NativeManagerEvidence.Exclusion] = []
            if hasStoreReceipt { exclusions.append(.appStoreReceipt) }
            if !references.isEmpty { exclusions.append(.homebrewCaskReference) }
            if applicationRoots.contains(where: { target.path.hasPrefix($0.appendingPathComponent("Setapp").path + "/") }) {
                exclusions.append(.setappLocation)
            }
            return NativeManagerEvidence(exclusions: exclusions, homebrewReferences: references,
                                         inspectedCaskEntries: entryCount)
        }
    }

    func snapshot(target: URL) throws -> Snapshot {
        var snapshot = Snapshot()
        for root in caskrooms {
            try scan(root, depth: 0, target: target, snapshot: &snapshot)
        }
        snapshot.references.sort()
        return snapshot
    }

    private func scan(_ directory: URL, depth: Int, target: URL, snapshot: inout Snapshot) throws {
        guard directory.path.utf8.count <= 4096 else { throw ObservationFailure.limitExceeded }
        let descriptor = open(directory.path, O_RDONLY | O_CLOEXEC | O_NOFOLLOW_ANY | O_DIRECTORY | O_NONBLOCK)
        if descriptor < 0 {
            if depth == 0, errno == ENOENT {
                snapshot.absentRoots.append(directory.path)
                return
            }
            throw ObservationFailure.unreadableManagerEvidence
        }
        guard let stream = fdopendir(descriptor) else {
            close(descriptor)
            throw ObservationFailure.unreadableManagerEvidence
        }
        defer { closedir(stream) }
        let before = try FileIdentity.read(descriptor: descriptor)
        snapshot.identities[directory.path] = before
        var children: [(URL, FileIdentity)] = []
        while true {
            errno = 0
            guard let entry = readdir(stream) else {
                guard errno == 0 else { throw ObservationFailure.unreadableManagerEvidence }
                break
            }
            let name = try entryName(entry)
            if name == "." || name == ".." { continue }
            guard snapshot.entryCount < entryLimit else { throw ObservationFailure.limitExceeded }
            snapshot.entryCount += 1
            let child = directory.appendingPathComponent(name)
            guard child.path.utf8.count <= 4096 else { throw ObservationFailure.limitExceeded }
            var status = stat()
            guard fstatat(descriptor, name, &status, AT_SYMLINK_NOFOLLOW) == 0 else {
                throw ObservationFailure.unreadableManagerEvidence
            }
            let identity = FileIdentity(status)
            snapshot.identities[child.path] = identity
            if depth == 2, identity.isSymlink {
                let destination = try linkDestination(descriptor: descriptor, name: name)
                snapshot.links[child.path] = destination
                // Compare the lexical absolute destination, not a basename or
                // bundle ID. Relative links are resolved only against this directory.
                let resolved = destination.hasPrefix("/") ? URL(fileURLWithPath: destination)
                    : directory.appendingPathComponent(destination)
                if resolved.standardizedFileURL.path == target.path {
                    snapshot.references.append(child.path)
                }
                var after = stat()
                guard fstatat(descriptor, name, &after, AT_SYMLINK_NOFOLLOW) == 0,
                      identity == FileIdentity(after) else { throw ObservationFailure.changedDuringObservation }
            } else if depth < 2, !name.hasPrefix(".") {
                guard identity.isDirectory else { throw ObservationFailure.unreadableManagerEvidence }
                children.append((child, identity))
            }
        }
        for (child, identity) in children.sorted(by: { $0.0.path < $1.0.path }) {
            try scan(child, depth: depth + 1, target: target, snapshot: &snapshot)
            guard snapshot.identities[child.path] == identity else { throw ObservationFailure.changedDuringObservation }
        }
        guard before == (try FileIdentity.read(descriptor: descriptor)),
              before == (try FileIdentity.read(directory)) else { throw ObservationFailure.changedDuringObservation }
    }

    private func entryName(_ entry: UnsafeMutablePointer<dirent>) throws -> String {
        let length = Int(entry.pointee.d_namlen)
        return try withUnsafeBytes(of: entry.pointee.d_name) { bytes in
            guard length > 0, length < bytes.count,
                  let name = String(bytes: bytes.prefix(length), encoding: .utf8),
                  !name.contains("/"), !name.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) }) else {
                throw ObservationFailure.unreadableManagerEvidence
            }
            return name
        }
    }

    private func linkDestination(descriptor: Int32, name: String) throws -> String {
        var bytes = [UInt8](repeating: 0, count: 4096)
        let byteCount = bytes.withUnsafeMutableBytes { readlinkat(descriptor, name, $0.baseAddress!, $0.count) }
        guard byteCount > 0, byteCount < bytes.count, let destination = String(bytes: bytes.prefix(byteCount), encoding: .utf8),
              !destination.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) }) else {
            throw ObservationFailure.unreadableManagerEvidence
        }
        return destination
    }
}
