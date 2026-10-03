import Darwin
import Foundation
import XCTest
@testable import HelmExternalUpdateObservation

final class HelperFilesystemTests: XCTestCase {
    private var root: URL!
    private var helper: URL!
    private var executable: URL!

    override func setUpWithError() throws {
        root = FileManager.default.homeDirectoryForCurrentUser.resolvingSymlinksInPath()
            .appendingPathComponent("helm-helper-filesystem-\(UUID().uuidString)")
        helper = root.appendingPathComponent("Updater.app")
        executable = helper.appendingPathComponent("Contents/MacOS/Updater")
        try FileManager.default.createDirectory(at: executable.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("fixture, not signed code".utf8).write(to: executable)
        XCTAssertEqual(chmod(root.path, 0o700), 0)
        XCTAssertEqual(chmod(executable.path, 0o755), 0)
    }

    override func tearDownWithError() throws {
        if let root { try FileManager.default.removeItem(at: root) }
    }

    private func reject(_ url: URL? = nil, file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertThrowsError(try HelperFilesystemSnapshot.capture(url ?? helper), file: file, line: line)
    }

    private func acl(_ permissions: String, at url: URL, tag: String = "allow") throws {
        let text = "!#acl 1\nuser:\(UUID().uuidString):::\(tag):\(permissions)\n"
        let value = try XCTUnwrap(acl_from_text(text))
        defer { acl_free(UnsafeMutableRawPointer(value)) }
        XCTAssertEqual(acl_set_file(url.path, ACL_TYPE_EXTENDED, value), 0)
    }

    func testStablePrivateTreeIncludesEveryAncestorAndDescendant() throws {
        let first = try HelperFilesystemSnapshot.capture(helper)
        XCTAssertEqual(first, try HelperFilesystemSnapshot.capture(helper))
        XCTAssertNotNil(first.ancestors["/"])
        XCTAssertNotNil(first.ancestors[root.path])
        XCTAssertNotNil(first.ancestors[root.deletingLastPathComponent().path])
        XCTAssertNotNil(first.tree.entries[helper.path])
        XCTAssertNotNil(first.tree.entries[executable.path])
        XCTAssertEqual(first.tree.entries.count, 4)
        XCTAssertFalse(first.tree.unsafePermissions)
    }

    func testWritableHelperRootIsRejected() {
        XCTAssertEqual(chmod(helper.path, 0o777), 0)
        reject()
    }

    func testWritableDescendantIsRejectedEvenWithSafeBundleRoot() {
        XCTAssertEqual(chmod(executable.path, 0o775), 0)
        reject()
    }

    func testWritableAncestorIsRejected() {
        XCTAssertEqual(chmod(root.path, 0o777), 0)
        reject()
    }

    func testParentAbovePrivateInstallationDirectoryIsNotSkipped() throws {
        let privateRoot = root.appendingPathComponent("Private")
        try FileManager.default.createDirectory(at: privateRoot, withIntermediateDirectories: true)
        let moved = privateRoot.appendingPathComponent("Updater.app")
        try FileManager.default.moveItem(at: helper, to: moved)
        XCTAssertEqual(chmod(privateRoot.path, 0o700), 0)
        XCTAssertEqual(chmod(root.path, 0o777), 0)
        reject(moved)
    }

    func testApplicationsNamedUserDirectoryDoesNotGetAdminException() throws {
        let applications = root.appendingPathComponent("Applications")
        try FileManager.default.createDirectory(at: applications, withIntermediateDirectories: true)
        let moved = applications.appendingPathComponent("Updater.app")
        try FileManager.default.moveItem(at: helper, to: moved)
        XCTAssertEqual(chmod(applications.path, 0o775), 0)
        reject(moved)
    }

    func testMutatingACLsRejectEachPermission() throws {
        for permission in ["write", "append", "delete", "writeattr", "writeextattr", "writesecurity", "chown"] {
            try acl(permission, at: executable)
            reject()
        }
        try acl("delete_child", at: helper)
        reject()
    }

    func testAncestorACLIsNotOverriddenByPrivateMode() throws {
        try acl("write", at: root)
        reject()
    }

    func testReadOnlyAndDenyACLsRemainSupported() throws {
        try acl("read", at: executable)
        XCTAssertNoThrow(try HelperFilesystemSnapshot.capture(helper))
        try acl("delete", at: root, tag: "deny")
        XCTAssertNoThrow(try HelperFilesystemSnapshot.capture(helper))
    }

    func testFinalAndIntermediateAliasesAreRejected() throws {
        let alias = root.appendingPathComponent("Alias.app")
        try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: helper)
        reject(alias)
        let parent = root.appendingPathComponent("AliasParent")
        try FileManager.default.createSymbolicLink(at: parent, withDestinationURL: root)
        reject(parent.appendingPathComponent("Updater.app"))
    }

    func testPermissionOpenRefusesIntermediateAliasEvenWhenInodeMatches() throws {
        let link = root.appendingPathComponent("Alias")
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: helper)
        let path = link.appendingPathComponent("Contents/MacOS/Updater")
        let identity = try FileIdentity.read(executable)
        XCTAssertEqual(identity, try FileIdentity.read(path))
        XCTAssertThrowsError(try BundleFilesystem().unsafePermissions(path, identity: identity)) { error in
            XCTAssertEqual(error as? ObservationFailure, .unreadablePermissions)
        }
    }

    func testContainedFrameworkStyleSymlinksAreSupported() throws {
        let framework = helper.appendingPathComponent("Contents/Frameworks/Example.framework")
        let version = framework.appendingPathComponent("Versions/A")
        try FileManager.default.createDirectory(at: version, withIntermediateDirectories: true)
        try Data("framework".utf8).write(to: version.appendingPathComponent("Example"))
        try FileManager.default.createSymbolicLink(atPath: framework.appendingPathComponent("Versions/Current").path,
                                                   withDestinationPath: "A")
        try FileManager.default.createSymbolicLink(atPath: framework.appendingPathComponent("Example").path,
                                                   withDestinationPath: "Versions/Current/Example")
        XCTAssertNoThrow(try HelperFilesystemSnapshot.capture(helper))
    }

    func testEscapingSymlinkIsRejected() throws {
        let link = helper.appendingPathComponent("Contents/outside")
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: root)
        reject()
    }

    func testDanglingSymlinkIsRejected() throws {
        let link = helper.appendingPathComponent("Contents/missing")
        try FileManager.default.createSymbolicLink(atPath: link.path, withDestinationPath: "absent")
        reject()
    }

    func testSymlinkCycleIsRejected() throws {
        let link = helper.appendingPathComponent("Contents/cycle")
        try FileManager.default.createSymbolicLink(atPath: link.path, withDestinationPath: "cycle")
        reject()
    }

    func testSpecialFilesFailWithoutBlocking() {
        let pipe = helper.appendingPathComponent("Contents/pipe")
        XCTAssertEqual(mkfifo(pipe.path, 0o600), 0)
        reject()
    }

    func testSetIDFilesAreRejected() {
        for mode: mode_t in [0o4755, 0o2755] {
            XCTAssertEqual(chmod(executable.path, mode), 0)
            reject()
        }
    }

    func testBoundedTreeAndNonDirectoryBundleFailClosed() throws {
        XCTAssertThrowsError(try HelperFilesystemSnapshot.capture(helper, entryLimit: 2))
        let file = root.appendingPathComponent("File.app")
        try Data("not a directory".utf8).write(to: file)
        reject(file)
    }

    func testSafeDescendantModeChangeIsPartOfSnapshot() throws {
        let before = try HelperFilesystemSnapshot.capture(helper)
        XCTAssertEqual(chmod(executable.path, 0o700), 0)
        XCTAssertNotEqual(before, try HelperFilesystemSnapshot.capture(helper))
    }

    func testSafeAncestorModeChangeIsPartOfSnapshot() throws {
        let before = try HelperFilesystemSnapshot.capture(helper)
        XCTAssertEqual(chmod(root.path, 0o755), 0)
        XCTAssertNotEqual(before, try HelperFilesystemSnapshot.capture(helper))
    }

    func testACLChangeIsPartOfSnapshotEvenWithoutMutationGrant() throws {
        let before = try HelperFilesystemSnapshot.capture(helper)
        try acl("read", at: executable)
        XCTAssertNotEqual(before, try HelperFilesystemSnapshot.capture(helper))
    }

    func testNewCallDoesNotReusePreviouslySafePermissions() throws {
        XCTAssertNoThrow(try HelperFilesystemSnapshot.capture(helper))
        XCTAssertEqual(chmod(executable.path, 0o777), 0)
        reject()
    }
}
