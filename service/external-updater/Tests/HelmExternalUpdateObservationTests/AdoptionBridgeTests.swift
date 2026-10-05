import CExternalUpdatePolicy
import Foundation
import XCTest
@testable import HelmExternalUpdateObservation

/// Synthetic evidence tests the ABI, not native peer authentication or grants
/// against installed apps. Each test uses a separate disposable ledger.
final class AdoptionBridgeTests: XCTestCase {
    private let applications = "/Users/agent/Applications"
    private let request = Data(#"{"schemaVersion":1,"consentId":"550e8400-e29b-41d4-a716-446655440000","targetPath":"/Applications/Example.app","expectedBundleIdentifier":"org.example.App","expectedInstalledBuild":"100"}"#.utf8)

    private func evidence(build: String = "100", major: Int = 2,
                          exclusions: [NativeManagerEvidence.Exclusion] = []) -> NativeTargetEvidence {
        NativeTargetEvidence(canonicalPath: "/Applications/Example.app", device: 42, inode: 123,
            bundleIdentifier: "org.example.App", build: build, teamIdentifier: "ABCDE12345",
            codeDirectoryHash: Array(repeating: 7, count: 20), ed25519PublicKey: Array(repeating: 8, count: 32),
            feedURL: "https://example.org/feed", frameworkMajor: major, hasStoreReceipt: false,
            writableByOthers: false, inspectedEntries: 10,
            managerEvidence: NativeManagerEvidence(exclusions: exclusions, homebrewReferences: [], inspectedCaskEntries: 2))
    }

    private func boundary(missing: Int? = nil, hash: UInt8 = 9) -> NativeAdoptionBoundary {
        NativeAdoptionBoundary(helperIdentifier: "com.jasoncavinder.Helm.SparkleExternalUpdater",
            helperTeamIdentifier: "V73WPJR9M4", helperCodeDirectoryHash: Data(repeating: hash, count: 20),
            callerIdentifier: "com.jasoncavinder.Helm", callerTeamIdentifier: "V73WPJR9M4",
            authenticatedLiveCaller: missing != 0, developerIDSignatureValid: missing != 1,
            notarizationAccepted: missing != 2, helmSandboxPreserved: missing != 3,
            externalHelperUnsandboxed: missing != 4, directConsumerChannel: missing != 5)
    }

    private func withLedger(_ operation: (PrivateLedgerDirectory) throws -> Void) throws {
        let root = FileManager.default.homeDirectoryForCurrentUser.resolvingSymlinksInPath()
            .appendingPathComponent("helm-adoption-bridge-test-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false, attributes: [.posixPermissions: 0o700])
        defer { try? FileManager.default.removeItem(at: root) }
        let scope = PrivateLedgerDirectory(home: root)
        try scope.withDatabase { try NativeHelperLedger.initialize(path: $0, fresh: $1) }
        try operation(scope)
    }

    private func prepare(_ path: String) throws -> NativeAdoptionReview {
        try NativeAdoptionReview(path: path, request: request, target: evidence(), boundary: boundary(),
                                 userApplications: applications, now: 100)
    }

    private func status(_ path: String) throws -> ExternalConsentStatus {
        let intent = try JSONEncoder().encode(ExternalPreflightRequest(targetPath: "/Applications/Example.app",
            bundleIdentifier: "org.example.App", installedBuild: "100"))
        return try NativeHelperLedger.inspect(path: path, evidence: evidence(), request: intent, userApplications: applications)
    }

    func testBoundaryLayoutAndEveryFlagReachTheRealBridge() throws {
        XCTAssertEqual(MemoryLayout<HelmExternalNativeBoundary>.size, 96)
        XCTAssertEqual(MemoryLayout<HelmExternalNativeBoundary>.alignment, 8)
        XCTAssertEqual(MemoryLayout<HelmExternalNativeBoundary>.offset(of: \.helper_code_directory_hash), 40)
        XCTAssertEqual(MemoryLayout<HelmExternalNativeBoundary>.offset(of: \.observed_flags), 88)
        boundary().withBoundary { XCTAssertEqual($0.pointee.observed_flags, 63) }
        try withLedger { scope in
            try scope.withDatabase(createIfMissing: false) { path, _ in
                for missing in 0..<6 {
                    let rejected = boundary(missing: missing)
                    rejected.withBoundary { XCTAssertEqual($0.pointee.observed_flags, 63 ^ (1 << missing)) }
                    XCTAssertThrowsError(try NativeAdoptionReview(path: path, request: request, target: evidence(),
                        boundary: rejected, userApplications: applications, now: 100))
                    let review = try prepare(path)
                    XCTAssertEqual(review.confirm(path: path, target: evidence(), boundary: rejected,
                        userApplications: applications, now: 110), .reviewChanged)
                    XCTAssertEqual(review.confirm(path: path, target: evidence(), boundary: boundary(),
                        userApplications: applications, now: 110), .reviewChanged)
                }
                XCTAssertEqual(try status(path), .notRecorded)
            }
        }
    }

    func testReviewIsReadOnlyConfirmationSingleUseAndOtherReviewStale() throws {
        try withLedger { scope in
            let database = scope.directory.appendingPathComponent("ledger.sqlite")
            try scope.withDatabase(createIfMissing: false) { path, _ in
                let before = try Data(contentsOf: database)
                let first = try prepare(path)
                let stale = try prepare(path)
                XCTAssertEqual(try Data(contentsOf: database), before)
                XCTAssertEqual(try status(path), .notRecorded)
                XCTAssertEqual(first.confirm(path: path, target: evidence(), boundary: boundary(), userApplications: applications, now: 110), .recorded)
                XCTAssertEqual(first.confirm(path: path, target: evidence(), boundary: boundary(), userApplications: applications, now: 110), .reviewChanged)
                XCTAssertEqual(stale.confirm(path: path, target: evidence(), boundary: boundary(), userApplications: applications, now: 110), .reviewChanged)
                XCTAssertEqual(try status(path), .recorded)
                XCTAssertEqual(NativePolicyAssessment.assess(evidence(), userApplications: applications), .unresolved)
            }
        }
    }

    func testSwiftMappingFailureAndAccountRootChangeStillConsumeHandle() throws {
        try withLedger { scope in
            try scope.withDatabase(createIfMissing: false) { path, _ in
                for invalidMapping in [false, true] {
                    let review = try prepare(path)
                    XCTAssertEqual(review.confirm(path: path, target: evidence(major: invalidMapping ? -1 : 2), boundary: boundary(),
                        userApplications: invalidMapping ? applications : nil, now: 110), .reviewChanged)
                    XCTAssertEqual(review.confirm(path: path, target: evidence(), boundary: boundary(),
                        userApplications: applications, now: 110), .reviewChanged)
                }
                XCTAssertEqual(try status(path), .notRecorded)
            }
        }
    }

    func testFreshChangesExpiryAndManagerOwnershipCannotGrant() throws {
        try withLedger { scope in
            try scope.withDatabase(createIfMissing: false) { path, _ in
                for target in [evidence(build: "101"), evidence(exclusions: [.homebrewCaskReference]),
                               evidence(exclusions: [.installerReceipt])] {
                    let review = try prepare(path)
                    XCTAssertEqual(review.confirm(path: path, target: target, boundary: boundary(), userApplications: applications, now: 110), .reviewChanged)
                }
                for now: UInt64 in [99, 220] {
                    XCTAssertEqual(try prepare(path).confirm(path: path, target: evidence(), boundary: boundary(),
                        userApplications: applications, now: now), .reviewChanged)
                }
                XCTAssertEqual(try prepare(path).confirm(path: path, target: evidence(), boundary: boundary(hash: 5),
                    userApplications: applications, now: 110), .reviewChanged)
                XCTAssertEqual(try status(path), .notRecorded)
            }
        }
    }

    func testRevocationAfterReviewCannotBeOverwritten() throws {
        try withLedger { scope in
            try scope.withDatabase(createIfMissing: false) { path, _ in
                let review = try prepare(path)
                let revoke = try NativeRevocationReview(path: path,
                    request: JSONEncoder().encode(ExternalRevocationRequest(targetPath: "/Applications/Example.app")), root: applications, now: 100)
                XCTAssertEqual(revoke.confirm(path: path, now: 110), .revoked)
                XCTAssertEqual(review.confirm(path: path, target: evidence(), boundary: boundary(), userApplications: applications, now: 110), .reviewChanged)
                XCTAssertEqual(try status(path), .revoked)
            }
        }
    }

    func testAbandonedReviewDoesNotGrantOrPreventLaterReview() throws {
        try withLedger { scope in
            try scope.withDatabase(createIfMissing: false) { path, _ in
                do { _ = try prepare(path) }
                XCTAssertEqual(try status(path), .notRecorded)
                XCTAssertEqual(try prepare(path).confirm(path: path, target: evidence(), boundary: boundary(), userApplications: applications, now: 110), .recorded)
            }
        }
    }

    func testMissingLedgerRejectedAndWrongPathConsumesReviewWithoutCreatingStorage() throws {
        try withLedger { scope in
            let missing = scope.home.appendingPathComponent("missing/ledger.sqlite").path
            XCTAssertThrowsError(try prepare(missing))
            try scope.withDatabase(createIfMissing: false) { path, _ in
                let review = try prepare(path)
                XCTAssertEqual(review.confirm(path: missing, target: evidence(), boundary: boundary(), userApplications: applications, now: 110), .reviewChanged)
                XCTAssertEqual(review.confirm(path: path, target: evidence(), boundary: boundary(), userApplications: applications, now: 110), .reviewChanged)
                XCTAssertEqual(try status(path), .notRecorded)
            }
            XCTAssertFalse(FileManager.default.fileExists(atPath: missing))
        }
    }

    func testOnlyDedicatedSuccessCodeMeansRecordedConsent() {
        XCTAssertEqual(NativeAdoptionOutcome(code: 51), .recorded)
        XCTAssertEqual(NativeAdoptionOutcome(code: 52), .reviewChanged)
        for code: UInt32 in [0, 1, 21, 39, 40, 41, 42, 53, .max] {
            XCTAssertEqual(NativeAdoptionOutcome(code: code), .outcomeUnknown)
        }
    }
}
