import Darwin
import Foundation

/// Local filesystem facts only. They do not establish manager provenance or
/// reserve a path against changes after observation.
struct BundleFilesystem {
    let entryLimit: Int

    init(entryLimit: Int = 100_000) {
        self.entryLimit = entryLimit
    }

    func ancestors(of target: URL) throws -> [String: FileIdentity] {
        var parent = target.deletingLastPathComponent()
        var identities: [String: FileIdentity] = [:]
        while true {
            let info = try FileIdentity.read(parent)
            guard info.isDirectory, info.owner == 0 || info.owner == geteuid(), info.mode & 0o002 == 0 else {
                throw ObservationFailure.unsafeOwnership
            }
            // Only the system Applications directory gets its normal admin
            // group mode exception; ACL mutation grants still fail closed.
            let allowAdminGroup = parent.path == "/Applications" && info.owner == 0
                && getgrnam("admin").map({ $0.pointee.gr_gid == info.group }) == true
            if try unsafePermissions(parent, identity: info, allowAdminGroup: allowAdminGroup) {
                throw ObservationFailure.unsafeOwnership
            }
            identities[parent.path] = info
            if parent.path == "/" { break }
            parent.deleteLastPathComponent()
        }
        return identities
    }

    struct TreeSnapshot: Equatable {
        let entries: [String: FileIdentity]
        let unsafePermissions: Bool
    }

    func tree(at target: URL) throws -> TreeSnapshot {
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
                let resolved = url.resolvingSymlinksInPath()
                guard resolved.path.hasPrefix(target.path + "/") else { throw ObservationFailure.outsideRoots }
                let destination = try FileIdentity.read(resolved)
                guard destination.isRegular || destination.isDirectory else { throw ObservationFailure.unsupportedFile }
                return
            }
            guard info.isRegular || info.isDirectory, info.mode & 0o6000 == 0 else {
                throw ObservationFailure.unsupportedFile
            }
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

    func unsafePermissions(_ url: URL, identity: FileIdentity, allowAdminGroup: Bool = false, rejectAllowACL: Bool = false) throws -> Bool {
        // O_NOFOLLOW protects only the final component. Refuse aliases anywhere
        // in the path when opening the object whose permissions were inspected.
        let descriptor = open(url.path, O_RDONLY | O_CLOEXEC | O_NOFOLLOW_ANY | O_NONBLOCK)
        guard descriptor >= 0 else { throw ObservationFailure.unreadablePermissions }
        defer { close(descriptor) }
        guard identity == (try FileIdentity.read(descriptor: descriptor)) else { throw ObservationFailure.changedDuringObservation }
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
                if rejectAllowACL { unsafe = true }
                var permissions: acl_permset_t?
                guard acl_get_permset(entry, &permissions) == 0, let permissions else { throw ObservationFailure.unreadablePermissions }
                for permission in [ACL_WRITE_DATA, ACL_APPEND_DATA, ACL_DELETE, ACL_DELETE_CHILD,
                                   ACL_WRITE_ATTRIBUTES, ACL_WRITE_EXTATTRIBUTES, ACL_WRITE_SECURITY, ACL_CHANGE_OWNER] {
                    let value = acl_get_perm_np(permissions, permission)
                    guard value >= 0 else { throw ObservationFailure.unreadablePermissions }
                    // Conservative even for owner-only grants: no inferred
                    // group membership or deny-precedence authorization.
                    unsafe = unsafe || value == 1
                }
            }
        }
        guard identity == (try FileIdentity.read(descriptor: descriptor)) else { throw ObservationFailure.changedDuringObservation }
        return unsafe
    }
}

struct FileIdentity: Equatable {
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
