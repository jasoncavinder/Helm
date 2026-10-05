import CExternalUpdatePolicy
import Darwin
import Foundation

public enum HelperLedgerFailure: Error, Equatable {
    case unavailable, unsafePath, changed, busy, incomplete, storageRejected
}

/// A separate, helper-owned namespace. Preparation is explicit; authenticated
/// history inspection only opens existing storage. Neither grants authority.
public struct NativeHelperLedger {
    private let identity: NativeHelperEvidence

    public init(identity: NativeHelperEvidence) {
        self.identity = identity
    }

    public func prepare() throws {
        guard try NativeHelperObserver().observeSelf() == identity,
              let applications = NativeApplicationRoots.userApplications else {
            throw HelperLedgerFailure.unavailable
        }
        let home = URL(fileURLWithPath: applications, isDirectory: true).deletingLastPathComponent()
        try PrivateLedgerDirectory(home: home).withDatabase { path, fresh in
            guard try NativeHelperObserver().observeSelf() == identity else { throw HelperLedgerFailure.changed }
            try Self.initialize(path: path, fresh: fresh)
            guard try NativeHelperObserver().observeSelf() == identity,
                  NativeApplicationRoots.userApplications == applications else { throw HelperLedgerFailure.changed }
        }
    }

    static func initialize(path: String, fresh: Bool) throws {
        let bytes = Data(path.utf8)
        let result = bytes.withUnsafeBytes { raw in
            helm_external_ledger_prepare(HelmExternalBytes(
                data: raw.bindMemory(to: UInt8.self).baseAddress, length: raw.count
            ), fresh ? 1 : 0)
        }
        guard result == 1 else { throw HelperLedgerFailure.storageRejected }
    }

    func consentStatus(evidence: NativeTargetEvidence, request: Data, userApplications: String?) throws -> ExternalConsentStatus {
        guard try NativeHelperObserver().observeSelf() == identity,
              let applications = NativeApplicationRoots.userApplications,
              applications == userApplications else { throw HelperLedgerFailure.unavailable }
        let home = URL(fileURLWithPath: applications, isDirectory: true).deletingLastPathComponent()
        return try PrivateLedgerDirectory(home: home).withDatabase(createIfMissing: false) { path, _ in
            let result = try Self.inspect(path: path, evidence: evidence, request: request, userApplications: applications)
            guard try NativeHelperObserver().observeSelf() == identity,
                  NativeApplicationRoots.userApplications == applications else { throw HelperLedgerFailure.changed }
            return result
        }
    }

    static func inspect(path: String, evidence: NativeTargetEvidence, request: Data,
                        userApplications: String?) throws -> ExternalConsentStatus {
        let path = Data(path.utf8)
        let code = NativePolicyAssessment.withTarget(evidence, userApplications: userApplications, invalid: UInt32(0)) { input in
            path.withUnsafeBytes { path in
                request.withUnsafeBytes { request in
                    helm_external_consent_status(input,
                        HelmExternalBytes(data: request.bindMemory(to: UInt8.self).baseAddress, length: request.count),
                        HelmExternalBytes(data: path.bindMemory(to: UInt8.self).baseAddress, length: path.count))
                }
            }
        }
        guard let result = ExternalConsentStatus(code: code) else { throw HelperLedgerFailure.storageRejected }
        return result
    }
}

/// Internal injection is for isolated filesystem tests. Runtime callers have no
/// HOME, database, root or environment override. All descriptors live through
/// the synchronous core operation; the lock serializes cooperating helpers.
struct PrivateLedgerDirectory {
    static let directoryName = "com.jasoncavinder.Helm.SparkleExternalUpdater"
    let home: URL

    var directory: URL {
        home.appendingPathComponent("Library", isDirectory: true)
            .appendingPathComponent("Application Support", isDirectory: true)
            .appendingPathComponent(Self.directoryName, isDirectory: true)
    }

    func withDatabase<T>(createIfMissing: Bool = true, _ operation: (String, Bool) throws -> T) throws -> T {
        guard getuid() != 0, getuid() == geteuid(), home.isFileURL,
              home.path.hasPrefix("/"), !home.path.contains("//"),
              !home.path.split(separator: "/").contains(where: { $0 == "." || $0 == ".." }) else {
            throw HelperLedgerFailure.unsafePath
        }
        let held = LedgerDescriptors()
        defer { withExtendedLifetime(held) {} }
        let homeFD = try held.openDirectory(home, privateMode: false)
        var parent = home
        var parentFD = homeFD
        for component in ["Library", "Application Support"] {
            parent.appendPathComponent(component, isDirectory: true)
            if createIfMissing, mkdirat(parentFD, component, 0o700) != 0, errno != EEXIST { throw HelperLedgerFailure.unavailable }
            parentFD = try held.openDirectory(parent, privateMode: false)
        }
        let fresh = createIfMissing && mkdirat(parentFD, Self.directoryName, 0o700) == 0
        guard !createIfMissing || fresh || errno == EEXIST else { throw HelperLedgerFailure.unavailable }
        let directoryFD = try held.openDirectory(directory, privateMode: true)
        // A partial previous initialization is recovery work. Never reconstruct a
        // missing lock or DB and silently forget a grant/revocation/reservation.
        let lock = directory.appendingPathComponent("ledger.lock", isDirectory: false)
        let lockFD = try held.openPrivateFile(lock, create: fresh)
        guard flock(lockFD, LOCK_EX | LOCK_NB) == 0 else { throw HelperLedgerFailure.busy }
        defer { flock(lockFD, LOCK_UN) }
        let database = directory.appendingPathComponent("ledger.sqlite", isDirectory: false)
        let databaseFD = try held.openPrivateFile(database, create: fresh)
        try validateFiles(directoryFD)
        try held.validate()
        let value = try operation(database.path, fresh)
        try held.validate()
        try validateFiles(directoryFD)
        // SQLite's core ledger commits request FULL/fullfsync. Also flush the
        // initial file and directory entries before acknowledging preparation.
        if createIfMissing {
            guard fcntl(databaseFD, F_FULLFSYNC) == 0, fsync(directoryFD) == 0,
                  fsync(parentFD) == 0 else { throw HelperLedgerFailure.unavailable }
        }
        try held.validate()
        return value
    }

    private func validateFiles(_ descriptor: Int32) throws {
        // Reopening gives an independent enumeration offset; dup shares offsets.
        let scanFD = open(directory.path, O_RDONLY | O_CLOEXEC | O_NOFOLLOW_ANY | O_DIRECTORY | O_NONBLOCK)
        guard scanFD >= 0 else { throw HelperLedgerFailure.unsafePath }
        guard let stream = fdopendir(scanFD) else { close(scanFD); throw HelperLedgerFailure.unavailable }
        defer { closedir(stream) }
        guard LedgerDescriptors.sameObject(try FileIdentity.read(descriptor: descriptor),
                                           try FileIdentity.read(descriptor: scanFD)) else { throw HelperLedgerFailure.changed }
        var count = 0
        while true {
            errno = 0
            guard let entry = readdir(stream) else {
                guard errno == 0 else { throw HelperLedgerFailure.unavailable }
                break
            }
            let name = try withUnsafeBytes(of: entry.pointee.d_name) { bytes -> String in
                let length = Int(entry.pointee.d_namlen)
                guard length > 0, length < bytes.count,
                      let name = String(bytes: bytes.prefix(length), encoding: .utf8), !name.contains("/") else {
                    throw HelperLedgerFailure.unsafePath
                }
                return name
            }
            if name == "." || name == ".." { continue }
            count += 1
            guard count <= 32, Self.allowedName(name) else { throw HelperLedgerFailure.unsafePath }
            let url = directory.appendingPathComponent(name, isDirectory: false)
            var before = stat()
            guard fstatat(descriptor, name, &before, AT_SYMLINK_NOFOLLOW) == 0,
                  FileIdentity(before).isRegular else { throw HelperLedgerFailure.unsafePath }
            let file = openat(descriptor, name, O_RDONLY | O_CLOEXEC | O_NOFOLLOW | O_NONBLOCK)
            guard file >= 0 else { throw HelperLedgerFailure.unsafePath }
            defer { close(file) }
            guard Self.fileIdentityMatches(before, descriptor: file) else { throw HelperLedgerFailure.changed }
            try LedgerDescriptors.validatePrivateFile(file, url: url)
        }
    }

    private static func fileIdentityMatches(_ before: stat, descriptor: Int32) -> Bool {
        (try? FileIdentity.read(descriptor: descriptor)) == FileIdentity(before)
    }

    static func allowedName(_ name: String) -> Bool {
        if ["ledger.lock", "ledger.sqlite", "ledger.sqlite-wal", "ledger.sqlite-shm", "ledger.sqlite-journal"].contains(name) { return true }
        // Core migrations retain verified private backups in the same directory.
        let prefix = "ledger.sqlite.pre-migration-v"
        guard name.hasPrefix(prefix), name.utf8.count < 128 else { return false }
        let suffix = name.hasSuffix(".backup.partial") ? ".backup.partial" : ".backup"
        guard name.hasSuffix(suffix) else { return false }
        let body = name.dropFirst(prefix.count).dropLast(suffix.count).split(separator: "-", omittingEmptySubsequences: false)
        return body.count == 2 && body.allSatisfy { !$0.isEmpty && $0.utf8.allSatisfy { $0 >= 48 && $0 <= 57 } }
    }
}

private final class LedgerDescriptors {
    private struct Held {
        let url: URL
        let descriptor: Int32
        let identity: FileIdentity
        let privateMode: Bool
        let directory: Bool
    }
    private var values = [Held]()

    deinit { for value in values.reversed() { close(value.descriptor) } }

    static func sameObject(_ lhs: FileIdentity, _ rhs: FileIdentity) -> Bool {
        lhs.device == rhs.device && lhs.inode == rhs.inode && lhs.owner == rhs.owner
            && lhs.group == rhs.group && lhs.mode == rhs.mode
    }

    func openDirectory(_ url: URL, privateMode: Bool) throws -> Int32 {
        let descriptor = open(url.path, O_RDONLY | O_CLOEXEC | O_NOFOLLOW_ANY | O_DIRECTORY | O_NONBLOCK)
        guard descriptor >= 0 else { throw HelperLedgerFailure.unsafePath }
        do {
            let identity = try FileIdentity.read(descriptor: descriptor)
            guard identity.isDirectory, identity.owner == geteuid(),
                  !privateMode || identity.mode & 0o7777 == 0o700,
                  try !BundleFilesystem().unsafePermissions(url, identity: identity, rejectAllowACL: privateMode) else { throw HelperLedgerFailure.unsafePath }
            _ = try BundleFilesystem().ancestors(of: url)
            values.append(Held(url: url, descriptor: descriptor, identity: identity, privateMode: privateMode, directory: true))
            return descriptor
        } catch { close(descriptor); throw error }
    }

    func openPrivateFile(_ url: URL, create: Bool) throws -> Int32 {
        let before = create ? nil : try FileIdentity.read(url)
        guard before == nil || before?.isRegular == true else { throw HelperLedgerFailure.unsafePath }
        let flags = O_RDWR | O_CLOEXEC | O_NOFOLLOW_ANY | O_NONBLOCK | (create ? O_CREAT | O_EXCL : 0)
        let descriptor = open(url.path, flags, 0o600)
        guard descriptor >= 0 else { throw HelperLedgerFailure.incomplete }
        do {
            try Self.validatePrivateFile(descriptor, url: url)
            let identity = try FileIdentity.read(descriptor: descriptor)
            guard before == nil || before == identity else { throw HelperLedgerFailure.changed }
            values.append(Held(url: url, descriptor: descriptor, identity: identity, privateMode: true, directory: false))
            return descriptor
        } catch { close(descriptor); throw error }
    }

    static func validatePrivateFile(_ descriptor: Int32, url: URL) throws {
        var info = stat()
        guard fstat(descriptor, &info) == 0, info.st_nlink == 1 else { throw HelperLedgerFailure.unsafePath }
        let identity = FileIdentity(info)
        guard identity.isRegular, identity.owner == geteuid(), identity.mode & 0o7777 == 0o600,
              identity.size >= 0, identity.size <= 256 * 1024 * 1024,
              try !BundleFilesystem().unsafePermissions(url, identity: identity, rejectAllowACL: true) else { throw HelperLedgerFailure.unsafePath }
    }

    func validate() throws {
        for value in values {
            let identity = try FileIdentity.read(descriptor: value.descriptor)
            guard Self.sameObject(value.identity, identity),
                  Self.sameObject(identity, try FileIdentity.read(value.url)) else { throw HelperLedgerFailure.changed }
            if value.directory {
                guard !value.privateMode || identity.mode & 0o7777 == 0o700,
                      try !BundleFilesystem().unsafePermissions(value.url, identity: identity, rejectAllowACL: value.privateMode) else { throw HelperLedgerFailure.unsafePath }
                _ = try BundleFilesystem().ancestors(of: value.url)
            } else {
                try Self.validatePrivateFile(value.descriptor, url: value.url)
            }
        }
    }
}
