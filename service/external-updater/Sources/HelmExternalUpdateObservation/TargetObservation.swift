import Darwin
import Foundation
import Security

public enum ObservationFailure: String, Error {
    case invalidPath, outsideRoots, unsupportedFile, changedDuringObservation
    case unsafeOwnership, unreadablePermissions, limitExceeded, invalidMetadata
    case invalidSignature, unsupportedSparkle, helmSelfUpdate
    case unreadableManagerEvidence
}

/// Locally collected facts, not installation authority. No Decodable initializer
/// exists: a future helper must never accept these facts from an XPC client.
public struct NativeTargetEvidence: Encodable, Equatable {
    public let canonicalPath: String
    public let device: UInt64
    public let inode: UInt64
    public let bundleIdentifier: String
    public let build: String
    public let teamIdentifier: String
    public let codeDirectoryHash: [UInt8]
    public let ed25519PublicKey: [UInt8]
    public let feedURL: String
    public let frameworkMajor: Int
    public let hasStoreReceipt: Bool
    public let writableByOthers: Bool
    public let inspectedEntries: Int
    public let managerEvidence: NativeManagerEvidence
    // File metadata and valid signatures do not establish manager ownership.
    public let requiresAuthorityResolution = true
}

struct NativeSigningEvidence {
    let identifier: String
    let team: String
    let hash: Data
    let info: [String: Any]
}

enum NativeApplicationRoots {
    static var userApplications: String? {
        getpwuid(geteuid()).map {
            URL(fileURLWithPath: String(cString: $0.pointee.pw_dir), isDirectory: true)
                .appendingPathComponent("Applications", isDirectory: true).path
        }
    }
}

public struct NativeTargetObserver {
    private let roots: [URL]
    private let signer: (URL) throws -> NativeSigningEvidence
    private let filesystem: BundleFilesystem
    private let managers: NativeManagerObserver
    private let receipts: NativeInstallerReceiptObserver
    private let macports: NativeMacPortsObserver

    public init() {
        // Resolve the account through the OS, not a caller-controlled HOME value.
        self.init(
            roots: [URL(fileURLWithPath: "/Applications")] + (NativeApplicationRoots.userApplications.map {
                [URL(fileURLWithPath: $0, isDirectory: true)]
            } ?? []),
            receipts: NativeInstallerReceiptObserver(), signer: Self.signingEvidence
        )
    }

    init(roots: [URL], entryLimit: Int = 100_000, managers: NativeManagerObserver = NativeManagerObserver(),
         receipts: NativeInstallerReceiptObserver, macports: NativeMacPortsObserver = NativeMacPortsObserver(),
         signer: @escaping (URL) throws -> NativeSigningEvidence) {
        self.roots = roots
        self.filesystem = BundleFilesystem(entryLimit: entryLimit)
        self.signer = signer
        self.managers = managers
        self.receipts = receipts
        self.macports = macports
    }

    public func observe(path: String) throws -> NativeTargetEvidence {
        let target = try validatedPath(path)
        let before = try FileIdentity.read(target)
        guard before.isDirectory else { throw ObservationFailure.unsupportedFile }
        guard roots.contains(where: { Self.contains($0, target) }) else { throw ObservationFailure.outsideRoots }
        let ancestors = try filesystem.ancestors(of: target)
        let permissions = try filesystem.tree(at: target)
        let managerSnapshot = try managers.snapshot(target: target)
        let macportsSnapshot = try macports.snapshot(target: target)
        let infoURL = target.appendingPathComponent("Contents/Info.plist")
        let infoBytes = try boundedRead(infoURL)
        guard let rawInfo = try PropertyListSerialization.propertyList(from: infoBytes, format: nil) as? [String: Any],
              let executable = rawInfo["CFBundleExecutable"] as? String,
              Self.bounded(executable, 255), executable != ".", executable != "..", !executable.contains("/"),
              permissions.entries[target.appendingPathComponent("Contents/MacOS", isDirectory: true)
                .appendingPathComponent(executable, isDirectory: false).path]?.isRegular == true else {
            throw ObservationFailure.invalidMetadata
        }
        let receiptPaths = Array(permissions.entries.keys)
        let receiptSnapshot = try receipts.snapshot(target: target, paths: receiptPaths)
        let frameworkURL = target.appendingPathComponent("Contents/Frameworks/Sparkle.framework/Resources/Info.plist")
            .resolvingSymlinksInPath()
        guard Self.contains(target, frameworkURL) else { throw ObservationFailure.unsupportedSparkle }
        let frameworkBytes = try boundedRead(frameworkURL)
        let signing = try signer(target)
        let info = signing.info
        guard let identifier = info["CFBundleIdentifier"] as? String,
              info["CFBundleExecutable"] as? String == executable,
              Self.bounded(identifier, 255), identifier == signing.identifier,
              let build = info["CFBundleVersion"] as? String, Self.bounded(build, 128),
              let feed = info["SUFeedURL"] as? String, Self.httpsURL(feed),
              let key = info["SUPublicEDKey"] as? String,
              let keyData = Data(base64Encoded: key), keyData.count == 32,
              signing.team.count == 10,
              signing.team.utf8.allSatisfy({ (65...90).contains($0) || (48...57).contains($0) }),
              [20, 32].contains(signing.hash.count), signing.hash.contains(where: { $0 != 0 }) else {
            throw ObservationFailure.invalidMetadata
        }
        guard !identifier.lowercased().hasPrefix("com.jasoncavinder.helm") else {
            throw ObservationFailure.helmSelfUpdate
        }
        guard let framework = try PropertyListSerialization.propertyList(from: frameworkBytes, format: nil) as? [String: Any],
              framework["CFBundleIdentifier"] as? String == "org.sparkle-project.Sparkle",
              let version = framework["CFBundleShortVersionString"] as? String,
              version.split(separator: ".").first == "2" else {
            throw ObservationFailure.unsupportedSparkle
        }
        // Finish external evidence queries before the final bundle snapshot so
        // target changes during a slow receipt query cannot escape revalidation.
        guard receiptSnapshot == (try receipts.snapshot(target: target, paths: receiptPaths)),
              managerSnapshot == (try managers.snapshot(target: target)) else {
            throw ObservationFailure.changedDuringObservation
        }
        let currentMacports = try macports.snapshot(target: target)
        // The signed Info.plist comes from Security.framework, not CFBundle's
        // mutable/cached dictionary. Re-observe after signature validation.
        guard before == (try FileIdentity.read(target)),
              infoBytes == (try boundedRead(infoURL)),
              frameworkBytes == (try boundedRead(frameworkURL)),
              target.path == target.resolvingSymlinksInPath().path else {
            throw ObservationFailure.changedDuringObservation
        }
        let afterPermissions = try filesystem.tree(at: target)
        guard permissions == afterPermissions,
              ancestors == (try filesystem.ancestors(of: target)) else {
            throw ObservationFailure.changedDuringObservation
        }
        guard macportsSnapshot == currentMacports else { throw ObservationFailure.changedDuringObservation }
        // Even an empty or aliased receipt container is an exclusion marker,
        // not proof that this app is standalone or that its receipt is valid.
        let hasStoreReceipt = permissions.entries[target.appendingPathComponent("Contents/_MASReceipt").path] != nil
        return NativeTargetEvidence(
            canonicalPath: target.path, device: before.device, inode: before.inode,
            bundleIdentifier: identifier, build: build, teamIdentifier: signing.team,
            codeDirectoryHash: Array(signing.hash), ed25519PublicKey: Array(keyData),
            feedURL: feed, frameworkMajor: 2,
            hasStoreReceipt: hasStoreReceipt,
            writableByOthers: permissions.unsafePermissions, inspectedEntries: permissions.entries.count,
            managerEvidence: managerSnapshot.evidence(target: target, applicationRoots: roots, hasStoreReceipt: hasStoreReceipt,
                                                     entries: permissions.entries, installerPackages: receiptSnapshot.identifiers,
                                                     macportsClaims: macportsSnapshot.claims)
        )
    }

    private func validatedPath(_ path: String) throws -> URL {
        guard Self.bounded(path, 4096), path.hasPrefix("/"), path.hasSuffix(".app"),
              !path.contains("//"), !path.split(separator: "/").contains(where: { $0 == "." || $0 == ".." }) else {
            throw ObservationFailure.invalidPath
        }
        let target = URL(fileURLWithPath: path)
        guard target.path == target.resolvingSymlinksInPath().path,
              roots.contains(where: { $0.path == $0.resolvingSymlinksInPath().path && Self.contains($0, target) }) else {
            throw ObservationFailure.outsideRoots
        }
        var parent = target.deletingLastPathComponent()
        while parent.path != "/" {
            guard parent.pathExtension.lowercased() != "app" else { throw ObservationFailure.outsideRoots }
            parent.deleteLastPathComponent()
        }
        return target
    }

    private func boundedRead(_ url: URL) throws -> Data {
        let descriptor = open(url.path, O_RDONLY | O_CLOEXEC | O_NOFOLLOW_ANY | O_NONBLOCK)
        guard descriptor >= 0 else { throw ObservationFailure.invalidMetadata }
        defer { close(descriptor) }
        let before = try FileIdentity.read(descriptor: descriptor)
        guard before.isRegular, before.size > 0, before.size <= 2 * 1024 * 1024 else { throw ObservationFailure.invalidMetadata }
        var bytes = [UInt8](repeating: 0, count: Int(before.size))
        var offset = 0
        while offset < bytes.count {
            let bytesRead = bytes.withUnsafeMutableBytes { read(descriptor, $0.baseAddress!.advanced(by: offset), $0.count - offset) }
            if bytesRead < 0, errno == EINTR { continue }
            guard bytesRead > 0 else { throw ObservationFailure.invalidMetadata }
            offset += bytesRead
        }
        guard before == (try FileIdentity.read(descriptor: descriptor)) else { throw ObservationFailure.changedDuringObservation }
        return Data(bytes)
    }

    static func signingEvidence(_ target: URL) throws -> NativeSigningEvidence {
        var code: SecStaticCode?
        guard SecStaticCodeCreateWithPath(target as CFURL, [], &code) == errSecSuccess, let code else {
            throw ObservationFailure.invalidSignature
        }
        var requirement: SecRequirement?
        let text = "anchor apple generic and certificate leaf[field.1.2.840.113635.100.6.1.13] exists"
        guard SecRequirementCreateWithString(text as CFString, [], &requirement) == errSecSuccess,
              let requirement else { throw ObservationFailure.invalidSignature }
        let flags = SecCSFlags(rawValue: kSecCSStrictValidate | kSecCSCheckAllArchitectures | kSecCSCheckNestedCode)
        // No allow-network flag, process execution, certificate installation or
        // developer-tools probe. This is local static code validation only.
        guard SecStaticCodeCheckValidity(code, flags, requirement) == errSecSuccess else { throw ObservationFailure.invalidSignature }
        var raw: CFDictionary?
        guard SecCodeCopySigningInformation(code, SecCSFlags(rawValue: kSecCSSigningInformation), &raw) == errSecSuccess,
              let values = raw as? [String: Any],
              let identifier = values[kSecCodeInfoIdentifier as String] as? String,
              let team = values[kSecCodeInfoTeamIdentifier as String] as? String,
              let hash = values[kSecCodeInfoUnique as String] as? Data,
              let info = values[kSecCodeInfoPList as String] as? [String: Any] else { throw ObservationFailure.invalidSignature }
        return NativeSigningEvidence(identifier: identifier, team: team, hash: hash, info: info)
    }

    private static func contains(_ root: URL, _ url: URL) -> Bool { url.path.hasPrefix(root.path + "/") }
    private static func bounded(_ value: String, _ limit: Int) -> Bool {
        !value.isEmpty && value.utf8.count <= limit && value.trimmingCharacters(in: .whitespacesAndNewlines) == value
            && !value.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) })
    }
    private static func httpsURL(_ value: String) -> Bool {
        guard bounded(value, 4096), value.hasPrefix("https://"), !value.contains("\\"),
              let url = URLComponents(string: value) else { return false }
        return url.scheme == "https" && url.host?.isEmpty == false && url.user == nil && url.password == nil
            && url.fragment == nil && (url.port == nil || url.port == 443)
    }
}
