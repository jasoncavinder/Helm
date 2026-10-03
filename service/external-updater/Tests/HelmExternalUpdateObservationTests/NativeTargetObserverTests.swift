import Darwin
import Foundation
import XCTest
@testable import HelmExternalUpdateObservation

final class NativeTargetObserverTests: XCTestCase {
    private var root: URL!
    private var target: URL!
    private var managers: NativeManagerObserver {
        NativeManagerObserver(caskrooms: [root.appendingPathComponent("Caskroom")])
    }

    override func setUpWithError() throws {
        // Shared temporary roots can contain aliases or writable ancestors;
        // use a private, home-rooted fixture like the supported app location.
        root = FileManager.default.homeDirectoryForCurrentUser.resolvingSymlinksInPath()
            .appendingPathComponent("helm-observer-\(UUID().uuidString)")
        target = root.appendingPathComponent("Example.app")
        let resources = target.appendingPathComponent("Contents/Frameworks/Sparkle.framework/Resources")
        try FileManager.default.createDirectory(at: resources, withIntermediateDirectories: true)
        XCTAssertEqual(chmod(root.path, 0o700), 0)
        try FileManager.default.createDirectory(at: target.appendingPathComponent("Contents/MacOS"), withIntermediateDirectories: true)
        try Data("fixture executable, never launched".utf8).write(to: target.appendingPathComponent("Contents/MacOS/Example"))
        try plist(signature().info).write(to: target.appendingPathComponent("Contents/Info.plist"))
        try plist([
            "CFBundleIdentifier": "org.sparkle-project.Sparkle",
            "CFBundleShortVersionString": "2.8.1"
        ]).write(to: resources.appendingPathComponent("Info.plist"))
    }

    override func tearDownWithError() throws {
        if let root { try FileManager.default.removeItem(at: root) }
    }

    private func plist(_ object: [String: Any]) throws -> Data {
        try PropertyListSerialization.data(fromPropertyList: object, format: .xml, options: 0)
    }

    private func signature(_ overrides: [String: Any] = [:]) -> NativeSigningEvidence {
        var info: [String: Any] = [
            "CFBundleIdentifier": "org.example.App", "CFBundleVersion": "100", "CFBundleExecutable": "Example",
            "SUFeedURL": "https://example.org/appcast.xml",
            "SUPublicEDKey": Data(repeating: 7, count: 32).base64EncodedString()
        ]
        info.merge(overrides) { _, new in new }
        return NativeSigningEvidence(identifier: info["CFBundleIdentifier"] as? String ?? "invalid",
                                     team: "ABCDE12345", hash: Data(repeating: 1, count: 20), info: info)
    }

    private func observer(_ overrides: [String: Any] = [:], limit: Int = 100_000) -> NativeTargetObserver {
        NativeTargetObserver(roots: [root], entryLimit: limit, managers: managers) { _ in self.signature(overrides) }
    }

    func testLocalEvidenceKeepsAuthorityUnresolved() throws {
        let evidence = try observer().observe(path: target.path)
        XCTAssertEqual(evidence.bundleIdentifier, "org.example.App")
        XCTAssertEqual(evidence.build, "100")
        XCTAssertEqual(evidence.frameworkMajor, 2)
        XCTAssertEqual(evidence.ed25519PublicKey.count, 32)
        XCTAssertFalse(evidence.hasStoreReceipt)
        XCTAssertFalse(evidence.writableByOthers)
        XCTAssertTrue(evidence.requiresAuthorityResolution)
        XCTAssertGreaterThan(evidence.inspectedEntries, 1)
        XCTAssertEqual(try observer().observe(path: target.path).inode, evidence.inode)
    }

    func testInvalidPathsNeverReachCodeValidator() throws {
        var calls = 0
        let observer = NativeTargetObserver(roots: [root], managers: managers) { _ in calls += 1; return self.signature() }
        for path in ["relative.app", target.path + "/", root.path + "/./Example.app",
                     root.path + "//Example.app", root.path + "/../Example.app", target.path + "\n", "/tmp/Unknown.app"] {
            XCTAssertThrowsError(try observer.observe(path: path), path)
        }
        XCTAssertEqual(calls, 0)
    }

    func testAliasAndNestedAppAreRejected() throws {
        let alias = root.appendingPathComponent("Alias.app")
        try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: target)
        XCTAssertThrowsError(try observer().observe(path: alias.path))
        let nested = target.appendingPathComponent("Contents/Nested.app")
        try FileManager.default.createDirectory(at: nested, withIntermediateDirectories: true)
        XCTAssertThrowsError(try observer().observe(path: nested.path))
    }

    func testEmbeddedSymlinkCannotEscapeBundle() throws {
        let link = target.appendingPathComponent("Contents/outside")
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: root)
        XCTAssertThrowsError(try observer().observe(path: target.path)) { error in
            XCTAssertEqual(error as? ObservationFailure, .outsideRoots)
        }
    }

    func testInternalFrameworkSymlinkIsAllowed() throws {
        let framework = target.appendingPathComponent("Contents/Frameworks/Sparkle.framework")
        let destination = framework.appendingPathComponent("Versions/A/Resources")
        try FileManager.default.createDirectory(at: destination.deletingLastPathComponent(), withIntermediateDirectories: true)
        try FileManager.default.moveItem(at: framework.appendingPathComponent("Resources"), to: destination)
        try FileManager.default.createSymbolicLink(at: framework.appendingPathComponent("Resources"), withDestinationURL: destination)
        XCTAssertEqual(try observer().observe(path: target.path).frameworkMajor, 2)
    }

    func testPermissionsAndReceiptAreFactsNotSilentEligibility() throws {
        let receipt = target.appendingPathComponent("Contents/_MASReceipt/receipt")
        try FileManager.default.createDirectory(at: receipt.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("receipt".utf8).write(to: receipt)
        XCTAssertEqual(chmod(receipt.path, 0o666), 0)
        let evidence = try observer().observe(path: target.path)
        XCTAssertTrue(evidence.hasStoreReceipt)
        XCTAssertTrue(evidence.writableByOthers)
        XCTAssertTrue(evidence.requiresAuthorityResolution)
    }

    func testGrantACLIsConservativelyUnsafe() throws {
        let file = target.appendingPathComponent("Contents/acl-file")
        try Data("test".utf8).write(to: file)
        let text = "!#acl 1\nuser:\(UUID().uuidString):::allow:write\n"
        guard let acl = acl_from_text(text) else { return XCTFail("ACL fixture could not be created") }
        defer { acl_free(UnsafeMutableRawPointer(acl)) }
        XCTAssertEqual(acl_set_file(file.path, ACL_TYPE_EXTENDED, acl), 0)
        XCTAssertTrue(try observer().observe(path: target.path).writableByOthers)
    }

    func testMutationDuringValidationIsRejected() throws {
        let observer = NativeTargetObserver(roots: [root], managers: managers) { _ in
            try Data("changed".utf8).write(to: self.target.appendingPathComponent("Contents/new-file"))
            return self.signature()
        }
        XCTAssertThrowsError(try observer.observe(path: target.path)) { error in
            XCTAssertEqual(error as? ObservationFailure, .changedDuringObservation)
        }
    }

    func testNonRegularFilesAndEntryLimitFailClosed() throws {
        XCTAssertThrowsError(try observer(limit: 2).observe(path: target.path)) { error in
            XCTAssertEqual(error as? ObservationFailure, .limitExceeded)
        }
        let fifo = target.appendingPathComponent("Contents/pipe")
        XCTAssertEqual(mkfifo(fifo.path, 0o600), 0)
        XCTAssertThrowsError(try observer().observe(path: target.path)) { error in
            XCTAssertEqual(error as? ObservationFailure, .unsupportedFile)
        }
    }

    func testUntrustedFeedKeyAndHelmIdentityFailClosed() throws {
        for feed in ["http://example.org/feed", "https://u:p@example.org/feed", "https://example.org:444/feed",
                     "https://example.org/feed#fragment", "https://example.org\\evil/feed"] {
            XCTAssertThrowsError(try observer(["SUFeedURL": feed]).observe(path: target.path))
        }
        XCTAssertThrowsError(try observer(["SUPublicEDKey": "invalid"]).observe(path: target.path))
        XCTAssertThrowsError(try observer(["CFBundleIdentifier": "com.jasoncavinder.Helm"]).observe(path: target.path)) { error in
            XCTAssertEqual(error as? ObservationFailure, .helmSelfUpdate)
        }
    }

    func testNativeValidatorRejectsUnsignedFixture() throws {
        XCTAssertThrowsError(try NativeTargetObserver.signingEvidence(target)) { error in
            XCTAssertEqual(error as? ObservationFailure, .invalidSignature)
        }
    }

    func testUnsafeAncestorIsRejectedBeforeSignatureValidation() throws {
        XCTAssertEqual(chmod(root.path, 0o777), 0)
        var calls = 0
        let observer = NativeTargetObserver(roots: [root], managers: managers) { _ in calls += 1; return self.signature() }
        XCTAssertThrowsError(try observer.observe(path: target.path)) { error in
            XCTAssertEqual(error as? ObservationFailure, .unsafeOwnership)
        }
        XCTAssertEqual(calls, 0)
    }

    func testOversizedAndUnsupportedFrameworkFailClosed() throws {
        let file = target.appendingPathComponent("Contents/Frameworks/Sparkle.framework/Resources/Info.plist")
        try Data(repeating: 0, count: 2 * 1024 * 1024 + 1).write(to: file)
        XCTAssertThrowsError(try observer().observe(path: target.path)) { error in
            XCTAssertEqual(error as? ObservationFailure, .invalidMetadata)
        }
        try plist(["CFBundleIdentifier": "org.sparkle-project.Sparkle", "CFBundleShortVersionString": "1.27"]).write(to: file)
        XCTAssertThrowsError(try observer().observe(path: target.path)) { error in
            XCTAssertEqual(error as? ObservationFailure, .unsupportedSparkle)
        }
    }

    func testPermissionMutationDuringValidationIsRejected() throws {
        let observer = NativeTargetObserver(roots: [root], managers: managers) { _ in
            XCTAssertEqual(chmod(self.target.path, 0o777), 0)
            return self.signature()
        }
        XCTAssertThrowsError(try observer.observe(path: target.path)) { error in
            XCTAssertEqual(error as? ObservationFailure, .changedDuringObservation)
        }
    }

    func testReadOnlyACLDoesNotPretendToGrantMutation() throws {
        let file = target.appendingPathComponent("Contents/Info.plist")
        guard let acl = acl_from_text("!#acl 1\nuser:\(UUID().uuidString):::allow:read\n") else {
            return XCTFail("ACL fixture could not be created")
        }
        defer { acl_free(UnsafeMutableRawPointer(acl)) }
        XCTAssertEqual(acl_set_file(file.path, ACL_TYPE_EXTENDED, acl), 0)
        XCTAssertFalse(try observer().observe(path: target.path).writableByOthers)
    }

    func testAncestorMutationGrantACLIsRejected() throws {
        guard let acl = acl_from_text("!#acl 1\nuser:\(UUID().uuidString):::allow:write\n") else {
            return XCTFail("ACL fixture could not be created")
        }
        defer { acl_free(UnsafeMutableRawPointer(acl)) }
        XCTAssertEqual(acl_set_file(root.path, ACL_TYPE_EXTENDED, acl), 0)
        XCTAssertThrowsError(try observer().observe(path: target.path)) { error in
            XCTAssertEqual(error as? ObservationFailure, .unsafeOwnership)
        }
    }

    func testAncestorPermissionsChangingDuringSignatureValidationFailClosed() throws {
        let observer = NativeTargetObserver(roots: [root], managers: managers) { _ in
            XCTAssertEqual(chmod(self.root.path, 0o777), 0)
            return self.signature()
        }
        XCTAssertThrowsError(try observer.observe(path: target.path)) { error in
            XCTAssertEqual(error as? ObservationFailure, .unsafeOwnership)
        }
    }

    func testAncestorACLChangingDuringSignatureValidationFailsClosed() throws {
        let observer = NativeTargetObserver(roots: [root], managers: managers) { _ in
            guard let acl = acl_from_text("!#acl 1\nuser:\(UUID().uuidString):::allow:write\n") else {
                throw ObservationFailure.unreadablePermissions
            }
            defer { acl_free(UnsafeMutableRawPointer(acl)) }
            XCTAssertEqual(acl_set_file(self.root.path, ACL_TYPE_EXTENDED, acl), 0)
            return self.signature()
        }
        XCTAssertThrowsError(try observer.observe(path: target.path)) { error in
            XCTAssertEqual(error as? ObservationFailure, .unsafeOwnership)
        }
    }

    func testSafeAncestorModeDriftRequiresFreshObservation() throws {
        XCTAssertEqual(chmod(root.path, 0o755), 0)
        let observer = NativeTargetObserver(roots: [root], managers: managers) { _ in
            XCTAssertEqual(chmod(self.root.path, 0o700), 0)
            return self.signature()
        }
        XCTAssertThrowsError(try observer.observe(path: target.path)) { error in
            XCTAssertEqual(error as? ObservationFailure, .changedDuringObservation)
        }
    }

    func testWritableParentAboveApplicationRootIsRejectedBeforeValidation() throws {
        let applicationRoot = root.appendingPathComponent("Applications")
        try FileManager.default.createDirectory(at: applicationRoot, withIntermediateDirectories: true)
        let moved = applicationRoot.appendingPathComponent("Example.app")
        try FileManager.default.moveItem(at: target, to: moved)
        target = moved
        XCTAssertEqual(chmod(root.path, 0o777), 0)
        var calls = 0
        let observer = NativeTargetObserver(roots: [applicationRoot], managers: managers) { _ in
            calls += 1
            return self.signature()
        }
        XCTAssertThrowsError(try observer.observe(path: target.path)) { error in
            XCTAssertEqual(error as? ObservationFailure, .unsafeOwnership)
        }
        XCTAssertEqual(calls, 0)
    }

    func testOversizedBundleInfoIsRejectedBeforeNativeValidation() throws {
        try Data(repeating: 0, count: 2 * 1024 * 1024 + 1).write(to: target.appendingPathComponent("Contents/Info.plist"))
        var calls = 0
        let observer = NativeTargetObserver(roots: [root], managers: managers) { _ in calls += 1; return self.signature() }
        XCTAssertThrowsError(try observer.observe(path: target.path)) { error in
            XCTAssertEqual(error as? ObservationFailure, .invalidMetadata)
        }
        XCTAssertEqual(calls, 0)
    }
}
