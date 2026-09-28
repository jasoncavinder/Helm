import Darwin
import Foundation
import Security

public enum ObservationFailure: String, Error {
    case invalidPath, outsideRoots, unsupportedFile, changedDuringObservation
    case unsafeOwnership, unreadablePermissions, limitExceeded, invalidMetadata
    case invalidSignature, unsupportedSparkle, helmSelfUpdate
}

/// Locally collected facts, not installation authority. No Decodable initializer
/// exists: a future helper must never accept these facts from an XPC client.
public struct NativeTargetEvidence: Encodable {
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
    // File metadata and valid signatures do not establish manager ownership.
    public let requiresAuthorityResolution = true
}

struct NativeSigningEvidence {
    let identifier: String
    let team: String
    let hash: Data
    let info: [String: Any]
}

public struct NativeTargetObserver {
    private let roots: [URL]
    private let signer: (URL) throws -> NativeSigningEvidence
    private let entryLimit: Int

    public init() {
        // Resolve the account through the OS, not a caller-controlled HOME value.
        let home = getpwuid(geteuid()).map { String(cString: $0.pointee.pw_dir) }
        self.init(
            roots: [URL(fileURLWithPath: "/Applications")] + (home.map {
                [URL(fileURLWithPath: $0).appendingPathComponent("Applications")]
            } ?? []),
            signer: Self.signingEvidence
        )
    }

    init(roots: [URL], entryLimit: Int = 100_000, signer: @escaping (URL) throws -> NativeSigningEvidence) {
        self.roots = roots
        self.entryLimit = entryLimit
        self.signer = signer
    }

    public func observe(path: String) throws -> NativeTargetEvidence {
        let target = try validatedPath(path)
        let before = try FileIdentity.read(target)
        guard before.isDirectory else { throw ObservationFailure.unsupportedFile }
        guard let root = roots.first(where: { Self.contains($0, target) }) else { throw ObservationFailure.outsideRoots }
        try checkAncestors(target, root: root)
        let permissions = try inspectTree(target)
        let infoURL = target.appendingPathComponent("Contents/Info.plist")
        let infoBytes = try boundedRead(infoURL)
        let frameworkURL = target.appendingPathComponent("Contents/Frameworks/Sparkle.framework/Resources/Info.plist")
            .resolvingSymlinksInPath()
        guard Self.contains(target, frameworkURL) else { throw ObservationFailure.unsupportedSparkle }
        let frameworkBytes = try boundedRead(frameworkURL)
        let signing = try signer(target)
        let info = signing.info
        guard let identifier = info["CFBundleIdentifier"] as? String,
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
        // The signed Info.plist comes from Security.framework, not CFBundle's
        // mutable/cached dictionary. Re-observe after signature validation.
        guard before == (try FileIdentity.read(target)),
              infoBytes == (try boundedRead(infoURL)),
              frameworkBytes == (try boundedRead(frameworkURL)),
              target.path == target.resolvingSymlinksInPath().path else {
            throw ObservationFailure.changedDuringObservation
        }
        let afterPermissions = try inspectTree(target)
        guard permissions == afterPermissions else { throw ObservationFailure.changedDuringObservation }
        return NativeTargetEvidence(
            canonicalPath: target.path, device: before.device, inode: before.inode,
            bundleIdentifier: identifier, build: build, teamIdentifier: signing.team,
            codeDirectoryHash: Array(signing.hash), ed25519PublicKey: Array(keyData),
            feedURL: feed, frameworkMajor: 2,
            hasStoreReceipt: FileManager.default.fileExists(atPath: target.appendingPathComponent("Contents/_MASReceipt/receipt").path),
            writableByOthers: permissions.unsafePermissions, inspectedEntries: permissions.entries.count
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

    private func checkAncestors(_ target: URL, root: URL) throws {
        var parent = target.deletingLastPathComponent()
        while Self.contains(root, parent) || parent.path == root.path {
            let info = try FileIdentity.read(parent)
            guard info.isDirectory, info.owner == 0 || info.owner == geteuid(), info.mode & 0o002 == 0 else {
                throw ObservationFailure.unsafeOwnership
            }
            // /Applications may be group-admin writable. Nested directories do
            // not inherit that exception and must not grant mutation via ACLs.
            let allowAdminGroup = parent.path == "/Applications" && info.owner == 0
                && getgrnam("admin").map({ $0.pointee.gr_gid == info.group }) == true
            if try unsafePermissions(parent, identity: info, allowAdminGroup: allowAdminGroup) {
                throw ObservationFailure.unsafeOwnership
            }
            if parent.path == root.path { break }
            parent.deleteLastPathComponent()
        }
    }

    private struct TreeSnapshot: Equatable {
        let entries: [String: FileIdentity]
        let unsafePermissions: Bool
    }

    private func inspectTree(_ target: URL) throws -> TreeSnapshot {
        var failed = false
        guard let enumerator = FileManager.default.enumerator(at: target, includingPropertiesForKeys: nil, options: [], errorHandler: { _, _ in
            failed = true
            return false
        }) else { throw ObservationFailure.unsupportedFile }
        var entries: [String: FileIdentity] = [:]
        var unsafe = false
        func inspect(_ url: URL) throws {
            guard entries.count < entryLimit else { throw ObservationFailure.limitExceeded }
            let info = try FileIdentity.read(url)
            guard info.owner == 0 || info.owner == geteuid() else { throw ObservationFailure.unsafeOwnership }
            entries[url.path] = info
            if info.isSymlink {
                guard Self.contains(target, url.resolvingSymlinksInPath()) else { throw ObservationFailure.outsideRoots }
                return
            }
            guard info.isRegular || info.isDirectory else { throw ObservationFailure.unsupportedFile }
            unsafe = try unsafePermissions(url, identity: info) || unsafe
        }
        try inspect(target)
        while let object = enumerator.nextObject() {
            guard let url = object as? URL else { throw ObservationFailure.unsupportedFile }
            try inspect(url)
        }
        guard !failed else { throw ObservationFailure.unsupportedFile }
        return TreeSnapshot(entries: entries, unsafePermissions: unsafe)
    }

    private func unsafePermissions(_ url: URL, identity: FileIdentity, allowAdminGroup: Bool = false) throws -> Bool {
        let descriptor = open(url.path, O_RDONLY | O_CLOEXEC | O_NOFOLLOW | O_NONBLOCK)
        guard descriptor >= 0 else { throw ObservationFailure.unreadablePermissions }
        defer { close(descriptor) }
        guard identity == (try FileIdentity.read(descriptor: descriptor)) else { throw ObservationFailure.changedDuringObservation }
        // acl_get_fd_np reports ENOENT when no ACL is attached. Query the
        // successfully fetched security descriptor instead of treating every
        // ENOENT as safe (or rejecting ordinary files with no ACL).
        guard let security = filesec_init() else { throw ObservationFailure.unreadablePermissions }
        defer { filesec_free(security) }
        var status = stat()
        guard fstatx_np(descriptor, &status, security) == 0,
              identity == FileIdentity(status) else { throw ObservationFailure.unreadablePermissions }
        var present: Int32 = 0
        guard filesec_query_property(security, FILESEC_ACL, &present) == 0 else { throw ObservationFailure.unreadablePermissions }
        var unsafe = identity.mode & (allowAdminGroup ? 0o002 : 0o022) != 0
        guard present != 0 else { return unsafe }
        var value: acl_t?
        guard filesec_get_property(security, FILESEC_ACL, &value) == 0, let acl = value else {
            throw ObservationFailure.unreadablePermissions
        }
        defer { acl_free(UnsafeMutableRawPointer(acl)) }
        guard acl_valid(acl) == 0 else { throw ObservationFailure.unreadablePermissions }
        var entry: acl_entry_t?
        var position = ACL_FIRST_ENTRY
        while true {
            let result = acl_get_entry(acl, Int32(position.rawValue), &entry)
            if result == -1, errno == EINVAL { break }
            guard result == 0, let entry else { throw ObservationFailure.unreadablePermissions }
            position = ACL_NEXT_ENTRY
            var tag = ACL_UNDEFINED_TAG
            guard acl_get_tag_type(entry, &tag) == 0 else { throw ObservationFailure.unreadablePermissions }
            if tag == ACL_EXTENDED_ALLOW {
                var permissions: acl_permset_t?
                guard acl_get_permset(entry, &permissions) == 0, let permissions else { throw ObservationFailure.unreadablePermissions }
                for permission in [ACL_WRITE_DATA, ACL_APPEND_DATA, ACL_DELETE, ACL_DELETE_CHILD,
                                   ACL_WRITE_ATTRIBUTES, ACL_WRITE_EXTATTRIBUTES, ACL_WRITE_SECURITY, ACL_CHANGE_OWNER] {
                    let value = acl_get_perm_np(permissions, permission)
                    guard value >= 0 else { throw ObservationFailure.unreadablePermissions }
                    // Conservative even for an owner-only grant: resolving all
                    // group membership and deny precedence is not this slice.
                    unsafe = unsafe || value == 1
                }
            }
        }
        return unsafe
    }

    private func boundedRead(_ url: URL) throws -> Data {
        let descriptor = open(url.path, O_RDONLY | O_CLOEXEC | O_NOFOLLOW | O_NONBLOCK)
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

private struct FileIdentity: Equatable {
    let device: UInt64
    let inode: UInt64
    let owner: uid_t
    let group: gid_t
    let mode: mode_t
    let size: off_t
    let modifiedSeconds: Int
    let modifiedNanoseconds: Int
    let changedSeconds: Int
    let changedNanoseconds: Int
    var isDirectory: Bool { mode & S_IFMT == S_IFDIR }
    var isRegular: Bool { mode & S_IFMT == S_IFREG }
    var isSymlink: Bool { mode & S_IFMT == S_IFLNK }

    static func read(_ url: URL) throws -> Self {
        var value = stat()
        guard lstat(url.path, &value) == 0 else { throw ObservationFailure.unsupportedFile }
        return Self(value)
    }
    static func read(descriptor: Int32) throws -> Self {
        var value = stat()
        guard fstat(descriptor, &value) == 0 else { throw ObservationFailure.unsupportedFile }
        return Self(value)
    }
    init(_ value: stat) {
        device = UInt64(bitPattern: Int64(value.st_dev)); inode = UInt64(value.st_ino)
        owner = value.st_uid; group = value.st_gid; mode = value.st_mode; size = value.st_size
        modifiedSeconds = value.st_mtimespec.tv_sec; modifiedNanoseconds = value.st_mtimespec.tv_nsec
        changedSeconds = value.st_ctimespec.tv_sec; changedNanoseconds = value.st_ctimespec.tv_nsec
    }
}
