import Darwin
import Foundation
import XCTest
@testable import HelmExternalUpdateObservation

final class HelperLedgerTests: XCTestCase {
    private var root: URL!
    private var scope: PrivateLedgerDirectory { PrivateLedgerDirectory(home: root) }
    private var database: URL { scope.directory.appendingPathComponent("ledger.sqlite") }

    override func setUpWithError() throws {
        root = FileManager.default.homeDirectoryForCurrentUser.resolvingSymlinksInPath()
            .appendingPathComponent("helm-ledger-test-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false, attributes: [.posixPermissions: 0o700])
    }

    override func tearDownWithError() throws {
        if let root { try FileManager.default.removeItem(at: root) }
    }

    private func prepare() throws {
        try scope.withDatabase { try NativeHelperLedger.initialize(path: $0, fresh: $1) }
    }

    private func privateFile(_ url: URL, bytes: Data = Data()) throws {
        try bytes.write(to: url)
        XCTAssertEqual(chmod(url.path, 0o600), 0)
    }

    func testPrivateInitializationAndReopenPreserveDatabaseIdentity() throws {
        try prepare()
        let first = try FileIdentity.read(database)
        XCTAssertGreaterThan(first.size, 0)
        XCTAssertEqual(first.mode & 0o777, 0o600)
        XCTAssertEqual(try FileIdentity.read(scope.directory).mode & 0o777, 0o700)
        try prepare()
        XCTAssertEqual(try FileIdentity.read(database).inode, first.inode)
    }

    func testPreparationDoesNotCreateAnyAdoptionOrSession() throws {
        try prepare()
        // Signed VM readback asserts the tables have no authority records.
        // Here verify the native boundary writes only the separate namespace.
        XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent("Library/Application Support/Helm/helm.db").path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent("Library/Application Support/Helm-Development/helm.db").path))
        XCTAssertEqual(Set(try FileManager.default.contentsOfDirectory(atPath: scope.directory.path)), ["ledger.lock", "ledger.sqlite"])
    }

    func testMissingExistingDatabaseIsNeverRecreated() throws {
        try prepare()
        try FileManager.default.removeItem(at: database)
        XCTAssertThrowsError(try prepare())
        XCTAssertFalse(FileManager.default.fileExists(atPath: database.path))
    }

    func testMissingExistingLockIsNeverRecreated() throws {
        try prepare()
        let lock = scope.directory.appendingPathComponent("ledger.lock")
        try FileManager.default.removeItem(at: lock)
        XCTAssertThrowsError(try prepare())
        XCTAssertFalse(FileManager.default.fileExists(atPath: lock.path))
    }

    func testExistingEmptyNamespaceFailsClosed() throws {
        try FileManager.default.createDirectory(at: scope.directory, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
        XCTAssertThrowsError(try prepare())
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: scope.directory.path), [])
    }

    func testTruncatedAndCorruptDatabaseAreNotReinitialized() throws {
        try prepare()
        for bytes in [Data(), Data("not a database".utf8)] {
            try privateFile(database, bytes: bytes)
            XCTAssertThrowsError(try prepare())
            XCTAssertEqual(try Data(contentsOf: database), bytes)
        }
    }

    func testNamespaceAndAncestorAliasesAreRejected() throws {
        let real = root.appendingPathComponent("Real", isDirectory: true)
        try FileManager.default.createDirectory(at: real, withIntermediateDirectories: false)
        let library = root.appendingPathComponent("Library", isDirectory: true)
        try FileManager.default.createSymbolicLink(at: library, withDestinationURL: real)
        XCTAssertThrowsError(try prepare())
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: real.path), [])
        try FileManager.default.removeItem(at: library)
        try FileManager.default.createDirectory(at: scope.directory.deletingLastPathComponent(), withIntermediateDirectories: true)
        try FileManager.default.createSymbolicLink(at: scope.directory, withDestinationURL: real)
        XCTAssertThrowsError(try prepare())
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: real.path), [])
    }

    func testDatabaseAliasCannotTouchDestination() throws {
        try prepare()
        let sentinel = root.appendingPathComponent("sentinel")
        try privateFile(sentinel, bytes: Data("unchanged".utf8))
        try FileManager.default.removeItem(at: database)
        try FileManager.default.createSymbolicLink(at: database, withDestinationURL: sentinel)
        XCTAssertThrowsError(try prepare())
        XCTAssertEqual(try Data(contentsOf: sentinel), Data("unchanged".utf8))
    }

    func testAliasedSidecarsAndBackupsCannotTouchDestinations() throws {
        try prepare()
        let sentinel = root.appendingPathComponent("sentinel")
        try privateFile(sentinel, bytes: Data("unchanged".utf8))
        for name in ["ledger.sqlite-wal", "ledger.sqlite-shm", "ledger.sqlite-journal", "ledger.sqlite.pre-migration-v24-123.backup"] {
            let sidecar = scope.directory.appendingPathComponent(name)
            try FileManager.default.createSymbolicLink(at: sidecar, withDestinationURL: sentinel)
            XCTAssertThrowsError(try prepare(), name)
            XCTAssertEqual(try Data(contentsOf: sentinel), Data("unchanged".utf8))
            try FileManager.default.removeItem(at: sidecar)
        }
    }

    func testGroupReadableOrWritableLedgerIsRejectedWithoutRepair() throws {
        try prepare()
        for mode: mode_t in [0o640, 0o660, 0o666] {
            XCTAssertEqual(chmod(database.path, mode), 0)
            XCTAssertThrowsError(try prepare())
            XCTAssertEqual(try FileIdentity.read(database).mode & 0o777, mode)
        }
    }

    func testNamespaceMustRemainOwnerOnly() throws {
        try prepare()
        for mode: mode_t in [0o750, 0o770, 0o777] {
            XCTAssertEqual(chmod(scope.directory.path, mode), 0)
            XCTAssertThrowsError(try prepare())
            XCTAssertEqual(try FileIdentity.read(scope.directory).mode & 0o777, mode)
        }
    }

    func testReadAndWriteAllowACLsCannotBypassPrivateModes() throws {
        try prepare()
        for permission in ["read", "write", "delete"] {
            let text = "!#acl 1\nuser:\(UUID().uuidString):::allow:\(permission)\n"
            let acl = try XCTUnwrap(acl_from_text(text))
            defer { acl_free(UnsafeMutableRawPointer(acl)) }
            XCTAssertEqual(acl_set_file(database.path, ACL_TYPE_EXTENDED, acl), 0)
            XCTAssertThrowsError(try prepare())
        }
    }

    func testMutationACLOnAncestorIsRejected() throws {
        try prepare()
        let text = "!#acl 1\nuser:\(UUID().uuidString):::allow:delete_child\n"
        let acl = try XCTUnwrap(acl_from_text(text))
        defer { acl_free(UnsafeMutableRawPointer(acl)) }
        XCTAssertEqual(acl_set_file(root.path, ACL_TYPE_EXTENDED, acl), 0)
        XCTAssertThrowsError(try prepare())
    }

    func testHardLinksAreRejectedForDatabaseAndSidecars() throws {
        try prepare()
        let extra = root.appendingPathComponent("hardlink")
        XCTAssertEqual(link(database.path, extra.path), 0)
        XCTAssertThrowsError(try prepare())
        try FileManager.default.removeItem(at: extra)
        let sidecar = scope.directory.appendingPathComponent("ledger.sqlite-wal")
        try privateFile(extra)
        XCTAssertEqual(link(extra.path, sidecar.path), 0)
        XCTAssertThrowsError(try prepare())
    }

    func testUnsupportedSidecarTypesAndNamesFailBeforeCore() throws {
        try prepare()
        for name in ["ledger.sqlite-wal", "unexpected-file"] {
            let url = scope.directory.appendingPathComponent(name)
            XCTAssertEqual(mkfifo(url.path, 0o600), 0)
            var entered = false
            XCTAssertThrowsError(try scope.withDatabase { _, _ in entered = true })
            XCTAssertFalse(entered)
            try FileManager.default.removeItem(at: url)
        }
    }

    func testConcurrentLeaseFailsWithoutBlockingAndCanRetry() throws {
        try prepare()
        try scope.withDatabase { _, _ in
            XCTAssertThrowsError(try prepare()) { XCTAssertEqual($0 as? HelperLedgerFailure, .busy) }
        }
        try prepare()
    }

    func testReplacementDuringOperationCannotReturnSuccess() throws {
        try prepare()
        XCTAssertThrowsError(try scope.withDatabase { _, _ in
            try FileManager.default.moveItem(at: database, to: scope.directory.appendingPathComponent("ledger.sqlite.pre-migration-v24-456.backup"))
            try privateFile(database)
        })
    }

    func testPermissionDriftDuringOperationCannotReturnSuccess() throws {
        try prepare()
        XCTAssertThrowsError(try scope.withDatabase { _, _ in XCTAssertEqual(chmod(database.path, 0o644), 0) })
    }

    func testPrivateBackupNamesAreStrictlyBounded() {
        for name in ["ledger.sqlite.pre-migration-v24-123.backup", "ledger.sqlite.pre-migration-v24-123.backup.partial"] {
            XCTAssertTrue(PrivateLedgerDirectory.allowedName(name))
        }
        for name in ["ledger.sqlite.pre-migration-v-123.backup", "ledger.sqlite.pre-migration-v24--.backup", "ledger.sqlite.pre-migration-v24-1e3.backup", "ledger.sqlite.pre-migration-v24-123.backup/secret"] {
            XCTAssertFalse(PrivateLedgerDirectory.allowedName(name))
        }
    }
}
