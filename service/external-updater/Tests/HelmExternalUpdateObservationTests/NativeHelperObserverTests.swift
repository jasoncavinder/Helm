import Darwin
import Foundation
import Security
import XCTest
@testable import HelmExternalUpdateObservation

final class NativeHelperObserverTests: XCTestCase {
    private func metadata() -> [String: Any] {
        [kSecCodeInfoIdentifier as String: "com.jasoncavinder.Helm.SparkleExternalUpdater",
         kSecCodeInfoTeamIdentifier as String: "V73WPJR9M4",
         kSecCodeInfoUnique as String: Data(repeating: 7, count: 20),
         kSecCodeInfoFlags as String: NSNumber(value: SecCodeSignatureFlags.runtime.rawValue),
         kSecCodeInfoPList as String: [
            "CFBundleIdentifier": "com.jasoncavinder.Helm.SparkleExternalUpdater",
            "CFBundleVersion": "1", "CFBundlePackageType": "APPL",
            "CFBundleExecutable": "HelmSparkleExternalUpdater",
            "HelmDistributionChannel": "developer_id"
         ]]
    }

    private func snapshot(path: String = "/Applications/HelmSparkleExternalUpdater.app",
                          inode: ino_t = 123, changed: Int = 10, values: [String: Any]? = nil) throws -> HelperSnapshot {
        var file = stat()
        file.st_dev = 1
        file.st_ino = inode
        file.st_ctimespec.tv_sec = changed
        return HelperSnapshot(path: path,
                              filesystem: HelperFilesystemSnapshot(ancestors: [:], tree: .init(
                                entries: [path: FileIdentity(file)], unsafePermissions: false)),
                              signature: try HelperSignature(values: values ?? metadata()))
    }

    private func replacingInfo(_ key: String, _ value: Any?) -> [String: Any] {
        var values = metadata()
        var info = values[kSecCodeInfoPList as String] as? [String: Any] ?? [:]
        info[key] = value
        values[kSecCodeInfoPList as String] = info
        return values
    }

    func testStableSnapshotProducesLocallyBoundedEvidence() throws {
        let fixture = try snapshot()
        var captures = 0
        let observer = NativeHelperObserver(testingCapture: { captures += 1; return fixture }, accounts: { (502, 502) })
        let result = try observer.observeSelf()
        XCTAssertEqual(captures, 2)
        XCTAssertEqual(result.canonicalPath, fixture.path)
        XCTAssertEqual(result.bundleIdentifier, fixture.signature.identifier)
        XCTAssertEqual(result.build, "1")
        XCTAssertEqual(result.teamIdentifier, "V73WPJR9M4")
        XCTAssertEqual(result.codeDirectoryHash, Array(repeating: 7, count: 20))
        XCTAssertEqual(result.account, 502)
        let data = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(result)) as? [String: Any])
        XCTAssertNil(data["authorized"])
        XCTAssertNil(data["notarized"])
        XCTAssertNil(data["eligible"])
    }

    func testRealObserverRejectsTheUnsignedTestHost() {
        XCTAssertThrowsError(try NativeHelperObserver().observeSelf()) { error in
            XCTAssertEqual(error as? HelperObservationFailure, .invalidSignature)
        }
    }

    func testRootAndSetuidFailBeforeSignatureCapture() {
        for account: (uid_t, uid_t) in [(0, 0), (502, 0), (0, 502), (502, 503)] {
            let observer = NativeHelperObserver(testingCapture: {
                XCTFail("invalid account reached code inspection")
                return try self.snapshot()
            }, accounts: { account })
            XCTAssertThrowsError(try observer.observeSelf()) { error in
                XCTAssertEqual(error as? HelperObservationFailure, .invalidAccount)
            }
        }
    }

    func testAccountDriftRejectsOtherwiseStableEvidence() throws {
        let fixture = try snapshot()
        var calls = 0
        let observer = NativeHelperObserver(testingCapture: { fixture }, accounts: {
            calls += 1
            return calls == 1 ? (502, 502) : (503, 503)
        })
        XCTAssertThrowsError(try observer.observeSelf()) { error in
            XCTAssertEqual(error as? HelperObservationFailure, .invalidAccount)
        }
    }

    func testSecondCaptureFailureDoesNotReuseEarlierEvidence() throws {
        let fixture = try snapshot()
        var calls = 0
        let observer = NativeHelperObserver(testingCapture: {
            calls += 1
            if calls == 2 { throw HelperObservationFailure.invalidSignature }
            return fixture
        })
        XCTAssertThrowsError(try observer.observeSelf()) { error in
            XCTAssertEqual(error as? HelperObservationFailure, .invalidSignature)
        }
    }

    func testIdentityAndFilesystemDriftRequireFreshObservation() throws {
        let original = try snapshot()
        var hashChanged = metadata()
        hashChanged[kSecCodeInfoUnique as String] = Data(repeating: 8, count: 20)
        let changes = try [snapshot(inode: 124), snapshot(changed: 11),
                           snapshot(path: "/Applications/Other.app"), snapshot(values: hashChanged),
                           snapshot(values: replacingInfo("CFBundleVersion", "2")),
                           snapshot(values: replacingInfo("CFBundleExecutable", "Other"))]
        for changed in changes {
            var calls = 0
            let observer = NativeHelperObserver(testingCapture: {
                calls += 1
                return calls == 1 ? original : changed
            })
            XCTAssertThrowsError(try observer.observeSelf()) { error in
                XCTAssertEqual(error as? HelperObservationFailure, .changedDuringObservation)
            }
        }
    }

    func testEveryCallRecollectsEvidenceWithoutCaching() throws {
        let fixture = try snapshot()
        var calls = 0
        let observer = NativeHelperObserver(testingCapture: {
            calls += 1
            if calls > 2 { throw HelperObservationFailure.invalidSignature }
            return fixture
        })
        _ = try observer.observeSelf()
        XCTAssertThrowsError(try observer.observeSelf())
        XCTAssertEqual(calls, 3)
    }

    func testAncestorAndDescendantDriftRejectsUnchangedCodeIdentity() throws {
        let original = try snapshot()
        var status = stat()
        status.st_ino = 456
        let identity = FileIdentity(status)
        var entries = original.filesystem.tree.entries
        entries[original.path + "/Contents/MacOS/Updater"] = identity
        let changedFilesystems = [
            HelperFilesystemSnapshot(ancestors: ["/Applications": identity], tree: original.filesystem.tree),
            HelperFilesystemSnapshot(ancestors: original.filesystem.ancestors,
                                     tree: .init(entries: entries, unsafePermissions: false))
        ]
        for filesystem in changedFilesystems {
            var captures = 0
            let observer = NativeHelperObserver(testingCapture: {
                captures += 1
                return captures == 1 ? original : HelperSnapshot(
                    path: original.path, filesystem: filesystem, signature: original.signature)
            })
            XCTAssertThrowsError(try observer.observeSelf()) { error in
                XCTAssertEqual(error as? HelperObservationFailure, .changedDuringObservation)
            }
        }
    }

    func testExactHelperAndTeamIdentityRequired() {
        for (key, value) in [(kSecCodeInfoIdentifier as String, "com.jasoncavinder.Helm"),
                             (kSecCodeInfoIdentifier as String, "com.jasoncavinder.Helm.SparkleExternalUpdater.other"),
                             (kSecCodeInfoTeamIdentifier as String, "OTHER12345")] {
            var values = metadata()
            values[key] = value
            XCTAssertThrowsError(try HelperSignature(values: values))
        }
    }

    func testSignedBundleMetadataCannotBeReplacedWithDisplayMetadata() {
        for (key, value) in [("CFBundleIdentifier", "other"), ("CFBundlePackageType", "XPC!"),
                             ("HelmDistributionChannel", "mas"), ("HelmDistributionChannel", "setapp"),
                             ("CFBundleVersion", ""), ("CFBundleVersion", "1\n"),
                             ("CFBundleVersion", String(repeating: "1", count: 129)),
                             ("CFBundleExecutable", "../Updater"), ("CFBundleExecutable", ".."),
                             ("CFBundleExecutable", ""), ("CFBundleExecutable", "Updater\0")] {
            XCTAssertThrowsError(try HelperSignature(values: replacingInfo(key, value)), "\(key): \(value)")
        }
        for key in ["CFBundleIdentifier", "CFBundleVersion", "CFBundlePackageType", "CFBundleExecutable", "HelmDistributionChannel"] {
            XCTAssertThrowsError(try HelperSignature(values: replacingInfo(key, nil)))
        }
    }

    func testMissingAndMalformedSigningMetadataFailsClosed() {
        for key in [kSecCodeInfoIdentifier, kSecCodeInfoTeamIdentifier, kSecCodeInfoUnique, kSecCodeInfoFlags, kSecCodeInfoPList] {
            var values = metadata()
            values.removeValue(forKey: key as String)
            XCTAssertThrowsError(try HelperSignature(values: values))
        }
        for hash in [Data(), Data(repeating: 0, count: 20), Data(repeating: 1, count: 19), Data(repeating: 1, count: 33)] {
            var values = metadata()
            values[kSecCodeInfoUnique as String] = hash
            XCTAssertThrowsError(try HelperSignature(values: values))
        }
    }

    func testRequiresHardenedRuntimeAndNoEntitlementGrants() throws {
        var values = metadata()
        values[kSecCodeInfoFlags as String] = NSNumber(value: 0)
        XCTAssertThrowsError(try HelperSignature(values: values)) { error in
            XCTAssertEqual(error as? HelperObservationFailure, .unsafeRuntime)
        }
        for key in ["com.apple.security.app-sandbox", "com.apple.security.inherit", "com.apple.security.get-task-allow",
                    "com.apple.security.cs.disable-library-validation", "com.apple.security.cs.allow-jit",
                    "com.apple.security.cs.allow-dyld-environment-variables", "unknown.future.entitlement"] {
            values = metadata()
            values[kSecCodeInfoEntitlementsDict as String] = [key: true]
            XCTAssertThrowsError(try HelperSignature(values: values))
        }
        values = metadata()
        values[kSecCodeInfoEntitlementsDict as String] = "malformed"
        XCTAssertThrowsError(try HelperSignature(values: values))
        values = metadata()
        values[kSecCodeInfoEntitlements as String] = Data([1])
        XCTAssertThrowsError(try HelperSignature(values: values))
        values = metadata()
        values[kSecCodeInfoEntitlementsDict as String] = [String: Any]()
        XCTAssertNoThrow(try HelperSignature(values: values))
    }
}
