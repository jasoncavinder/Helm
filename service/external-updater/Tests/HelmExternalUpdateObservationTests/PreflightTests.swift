import Darwin
import Foundation
import Security
import XCTest
@testable import HelmExternalUpdateObservation

#if DEBUG
final class PreflightTests: XCTestCase {
    private let token = Data(repeating: 9, count: 32)
    private var request: ExternalPreflightRequest {
        ExternalPreflightRequest(targetPath: "/Applications/Example.app", bundleIdentifier: "org.example.App", installedBuild: "100")
    }

    func testGateRequiresHandshakeExactSessionSequenceAndSize() throws {
        var absent = PreflightGate(started: 10)
        XCTAssertThrowsError(try absent.begin(token: token, sequence: 1, bytes: 1, now: 11))
        for (session, sequence, bytes) in [(Data(), UInt64(1), 1), (Data(repeating: 8, count: 32), 1, 1),
                                           (token, 0, 1), (token, 2, 1), (token, 1, 0), (token, 1, 8193)] {
            var gate = PreflightGate(started: 10)
            gate.establish(token)
            XCTAssertThrowsError(try gate.begin(token: session, sequence: sequence, bytes: bytes, now: 11))
        }
    }

    func testGateRejectsConcurrencyReplayAndLifetimeDrift() throws {
        var gate = PreflightGate(started: 10)
        gate.establish(token)
        try gate.begin(token: token, sequence: 1, bytes: 8192, now: 11)
        XCTAssertThrowsError(try gate.begin(token: token, sequence: 2, bytes: 1, now: 12))
        XCTAssertFalse(gate.complete(sequence: 2, now: 12))
        XCTAssertTrue(gate.complete(sequence: 1, now: 12))
        XCTAssertThrowsError(try gate.begin(token: token, sequence: 1, bytes: 1, now: 13))
        for now in [UInt64(9), 10 + BootstrapWire.lifetimeNanoseconds, UInt64.max] {
            XCTAssertThrowsError(try gate.begin(token: token, sequence: 2, bytes: 1, now: now))
        }
        try gate.begin(token: token, sequence: 2, bytes: 1, now: 14)
        XCTAssertFalse(gate.complete(sequence: 2, now: 10 + BootstrapWire.lifetimeNanoseconds))
        gate.close()
        gate.establish(token)
        XCTAssertFalse(gate.complete(sequence: 2, now: 15))
        XCTAssertThrowsError(try gate.begin(token: token, sequence: 3, bytes: 1, now: 15))
    }

    func testSessionWorkIsBoundedAndResultsAreNotPermissions() throws {
        var gate = PreflightGate(started: 10)
        gate.establish(token)
        for sequence in 1...PreflightGate.maximumRequests {
            try gate.begin(token: token, sequence: sequence, bytes: 1, now: 11)
            XCTAssertTrue(gate.complete(sequence: sequence, now: 12))
        }
        XCTAssertThrowsError(try gate.begin(token: token, sequence: 9, bytes: 1, now: 13))
        XCTAssertEqual(NativePolicyAssessment(code: 99), .internalFailure)
    }

    func testLateOrBackwardsResultsFailEvenBeforeTheTimeoutCallbackRuns() throws {
        for now in [UInt64(10), 11 + PreflightGate.requestNanoseconds, UInt64.max] {
            var gate = PreflightGate(started: 10)
            gate.establish(token)
            try gate.begin(token: token, sequence: 1, bytes: 1, now: 11)
            XCTAssertFalse(gate.complete(sequence: 1, now: now))
        }
    }

    func testStrictRustRequestValidationPrecedesNativeObservation() throws {
        let valid = try JSONEncoder().encode(request)
        _ = try ExternalPreflightRequest.decode(valid, userApplications: nil)
        var json = try XCTUnwrap(JSONSerialization.jsonObject(with: valid) as? [String: Any])
        for (key, value) in [("feedURL", "https://example.org/forged"), ("authority", "Standalone"),
                             ("databasePath", "/tmp/grant.db"), ("userApplicationsRoot", "/tmp")] {
            var bad = json
            bad[key] = value
            XCTAssertThrowsError(try ExternalPreflightRequest.decode(JSONSerialization.data(withJSONObject: bad), userApplications: nil))
        }
        json["targetPath"] = "/tmp/Example.app"
        let invalid = try JSONSerialization.data(withJSONObject: json)
        let identity = try helperIdentity()
        let processor = NativePreflightProcessor(identity: identity, helper: {
            XCTFail("invalid request reached native helper observation"); return identity
        }, target: { _ in XCTFail("invalid request reached filesystem"); return self.target() }, userApplications: { nil })
        XCTAssertThrowsError(try processor.assess(invalid))
        XCTAssertThrowsError(try processor.assess(Data(repeating: 32, count: 8193)))
        XCTAssertThrowsError(try processor.assess(Data([0xff])))
    }

    func testFreshIdentityExclusionsAndBuildBindingAcrossRealBridge() throws {
        let data = try JSONEncoder().encode(request)
        let identity = try helperIdentity()
        var helperReads = 0
        var targetReads = 0
        var target = self.target()
        let processor = NativePreflightProcessor(identity: identity, helper: { helperReads += 1; return identity },
            target: { path in XCTAssertEqual(path, self.request.targetPath); targetReads += 1; return target }, userApplications: { nil })
        XCTAssertEqual(try processor.assess(data), .unresolved)
        target = self.target(exclusions: [.homebrewCaskReference])
        XCTAssertEqual(try processor.assess(data), .otherManager)
        target = self.target(build: "101")
        XCTAssertEqual(try processor.assess(data), .targetChanged)
        XCTAssertEqual(helperReads, 6)
        XCTAssertEqual(targetReads, 3)
    }

    func testHelperDriftAndFailedCollectionNeverPublishOldEvidence() throws {
        let data = try JSONEncoder().encode(request)
        let identity = try helperIdentity()
        let changed = try helperIdentity(build: "2")
        for driftAt in [1, 2] {
            var calls = 0
            let processor = NativePreflightProcessor(identity: identity, helper: {
                calls += 1; return calls == driftAt ? changed : identity
            }, target: { _ in self.target() }, userApplications: { nil })
            XCTAssertThrowsError(try processor.assess(data))
        }
        let failed = NativePreflightProcessor(identity: identity, helper: { identity }, target: { _ in
            throw ObservationFailure.changedDuringObservation
        }, userApplications: { nil })
        XCTAssertEqual(try failed.assess(data), .observationFailed)
        var roots = 0
        let rootDrift = NativePreflightProcessor(identity: identity, helper: { identity }, target: { _ in self.target() },
            userApplications: { roots += 1; return roots == 1 ? nil : "/Users/agent/Applications" })
        XCTAssertThrowsError(try rootDrift.assess(data))
    }

    private func target(build: String = "100", exclusions: [NativeManagerEvidence.Exclusion] = []) -> NativeTargetEvidence {
        NativeTargetEvidence(canonicalPath: request.targetPath, device: 1, inode: 2,
                             bundleIdentifier: request.expectedBundleIdentifier, build: build,
                             teamIdentifier: "ABCDE12345", codeDirectoryHash: Array(repeating: 7, count: 20),
                             ed25519PublicKey: Array(repeating: 8, count: 32), feedURL: "https://example.org/feed",
                             frameworkMajor: 2, hasStoreReceipt: false, writableByOthers: false, inspectedEntries: 10,
                             managerEvidence: NativeManagerEvidence(exclusions: exclusions, homebrewReferences: [], inspectedCaskEntries: 1))
    }

    private func helperIdentity(build: String = "1") throws -> NativeHelperEvidence {
        let identifier = "com.jasoncavinder.Helm.SparkleExternalUpdater"
        let signature = try HelperSignature(values: [
            kSecCodeInfoIdentifier as String: identifier, kSecCodeInfoTeamIdentifier as String: "V73WPJR9M4",
            kSecCodeInfoUnique as String: Data(repeating: 7, count: 20),
            kSecCodeInfoFlags as String: NSNumber(value: SecCodeSignatureFlags.runtime.rawValue),
            kSecCodeInfoPList as String: ["CFBundleIdentifier": identifier, "CFBundleVersion": build,
                "CFBundlePackageType": "APPL", "CFBundleExecutable": "Updater", "HelmDistributionChannel": "developer_id"]])
        let snapshot = HelperSnapshot(path: "/Applications/Updater.app", filesystem: HelperFilesystemSnapshot(
            ancestors: [:], tree: .init(entries: [:], unsafePermissions: false)), signature: signature)
        return try NativeHelperObserver(testingCapture: { snapshot }).observeSelf()
    }
}
#endif
