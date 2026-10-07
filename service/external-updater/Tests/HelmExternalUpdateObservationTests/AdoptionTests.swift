import Foundation
import XCTest
@testable import HelmExternalUpdateObservation

final class AdoptionTests: XCTestCase {
    private let request = ExternalAdoptionRequest(targetPath: "/Applications/Example.app", bundleIdentifier: "org.example.App",
        installedBuild: "100", confirmsNoUnsupportedOwner: true)
    private let applications = "/Users/agent/Applications"

    private func snapshot(complete: Bool = true, build: String = "100",
                          gaps: [NativeCaskCoverageGap] = []) -> NativeAdoptionObservation {
        let target = NativeTargetEvidence(canonicalPath: request.targetPath, device: 42, inode: 123,
            bundleIdentifier: request.expectedBundleIdentifier, build: build, teamIdentifier: "ABCDE12345",
            codeDirectoryHash: Array(repeating: 7, count: 20), ed25519PublicKey: Array(repeating: 8, count: 32),
            feedURL: "https://example.org/feed", frameworkMajor: 2, hasStoreReceipt: false,
            writableByOthers: false, inspectedEntries: 10,
            managerEvidence: NativeManagerEvidence(exclusions: [], homebrewReferences: [], inspectedCaskEntries: 2, homebrewCoverageGaps: gaps))
        let boundary = NativeAdoptionBoundary(helperIdentifier: "com.jasoncavinder.Helm.SparkleExternalUpdater",
            helperTeamIdentifier: "V73WPJR9M4", helperCodeDirectoryHash: Data(repeating: 9, count: 20),
            callerIdentifier: "com.jasoncavinder.Helm", callerTeamIdentifier: "V73WPJR9M4",
            authenticatedLiveCaller: true, developerIDSignatureValid: true, notarizationAccepted: true,
            helmSandboxPreserved: true, externalHelperUnsandboxed: true, directConsumerChannel: true)
        return NativeAdoptionObservation(target: target, boundary: boundary, ownershipComplete: complete)
    }

    private func withLedger(_ operation: (PrivateLedgerDirectory) throws -> Void) throws {
        let home = FileManager.default.homeDirectoryForCurrentUser.resolvingSymlinksInPath()
            .appendingPathComponent("helm-adoption-transport-test-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: home, withIntermediateDirectories: false, attributes: [.posixPermissions: 0o700])
        defer { try? FileManager.default.removeItem(at: home) }
        let scope = PrivateLedgerDirectory(home: home)
        try scope.withDatabase { try NativeHelperLedger.initialize(path: $0, fresh: $1) }
        try operation(scope)
    }

    private func processor(_ scope: PrivateLedgerDirectory,
                           observe: (() throws -> NativeAdoptionObservation)? = nil) -> NativeAdoptionProcessor {
        NativeAdoptionProcessor(observe: { _ in try observe?() ?? self.snapshot() },
            userApplications: { self.applications }, scope: { _ in scope }, seconds: { 100 })
    }

    private func status(_ scope: PrivateLedgerDirectory) throws -> ExternalConsentStatus {
        let intent = try JSONEncoder().encode(ExternalPreflightRequest(targetPath: request.targetPath,
            bundleIdentifier: request.expectedBundleIdentifier, installedBuild: "100"))
        return try scope.withDatabase(createIfMissing: false) { path, _ in
            try NativeHelperLedger.inspect(path: path, evidence: snapshot().target, request: intent, userApplications: applications)
        }
    }

    func testStrictIntentCannotSupplyAuthorityStorageOrBoundary() throws {
        let data = try JSONEncoder().encode(request)
        XCTAssertNoThrow(try ExternalAdoptionRequest.decode(data, root: nil))
        for key in ["authority", "boundary", "databasePath", "ownershipComplete", "confirmed", "epoch"] {
            var object = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
            object[key] = true
            XCTAssertThrowsError(try ExternalAdoptionRequest.decode(JSONSerialization.data(withJSONObject: object), root: nil))
        }
        for path in ["/tmp/Example.app", "/Applications/../Example.app", "/Applications/Host.app/Example.app"] {
            let intent = ExternalAdoptionRequest(targetPath: path, bundleIdentifier: "org.example.App",
                installedBuild: "100", confirmsNoUnsupportedOwner: true)
            XCTAssertThrowsError(try ExternalAdoptionRequest.decode(JSONEncoder().encode(intent), root: nil))
        }
        XCTAssertThrowsError(try ExternalAdoptionRequest.decode(Data(repeating: 0, count: 8193), root: nil))
    }

    func testScopeAcknowledgmentIsExplicitAndRejectedBeforeNativeWorkWhenInvalid() throws {
        let valid = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(request)) as? [String: Any])
        var observations = 0
        let processor = NativeAdoptionProcessor(observe: { _ in observations += 1; return self.snapshot() },
            userApplications: { self.applications })
        for (key, value) in [("ownershipScopeVersion", 0 as Any), ("ownershipScopeVersion", 2 as Any),
                             ("confirmsNoUnsupportedOwner", false as Any), ("schemaVersion", 1 as Any),
                             ("ownershipScopeVersion", NSNull()), ("confirmsNoUnsupportedOwner", NSNull())] {
            var object = valid
            object[key] = value
            XCTAssertThrowsError(try processor.prepare(JSONSerialization.data(withJSONObject: object)))
        }
        for key in ["ownershipScopeVersion", "confirmsNoUnsupportedOwner"] {
            var object = valid
            object.removeValue(forKey: key)
            XCTAssertThrowsError(try processor.prepare(JSONSerialization.data(withJSONObject: object)))
        }
        let declined = ExternalAdoptionRequest(targetPath: request.targetPath, bundleIdentifier: "org.example.App",
            installedBuild: "100", confirmsNoUnsupportedOwner: false)
        XCTAssertThrowsError(try processor.prepare(JSONEncoder().encode(declined)))
        XCTAssertEqual(observations, 0)
    }

    func testScopeChangedHistoryCodeRoundTripsWithoutAuthority() {
        XCTAssertEqual(ExternalConsentStatus(code: 26), .scopeChanged)
        XCTAssertEqual(ExternalConsentStatus.scopeChanged.code, 26)
        XCTAssertNil(ExternalConsentStatus(code: 27))
    }

    func testCoordinatorHandlesAreConnectionLocalAndConsumedByAnyAttempt() throws {
        var writes = 0
        let prepare: (Data) throws -> PreparedAdoption = { _ in PreparedAdoption { admit in try admit(); writes += 1; return .recorded } }
        let first = AdoptionCoordinator(prepare: prepare)
        let other = AdoptionCoordinator(prepare: prepare)
        let data = try JSONEncoder().encode(request)
        let old = try first.review(data)
        let fresh = try first.review(data)
        XCTAssertNotEqual(old, fresh)
        XCTAssertThrowsError(try other.confirm(fresh) {})
        XCTAssertThrowsError(try first.confirm(old) {})
        XCTAssertThrowsError(try first.confirm(fresh) {})
        let current = try first.review(data)
        XCTAssertEqual(try first.confirm(current) {}, .recorded)
        XCTAssertThrowsError(try first.confirm(current) {})
        let replaced = try first.review(data)
        XCTAssertThrowsError(try first.review(Data("{}".utf8)))
        XCTAssertThrowsError(try first.confirm(replaced) {})
        let cleared = try first.review(data)
        first.clear()
        XCTAssertThrowsError(try first.confirm(cleared) {})
        XCTAssertEqual(writes, 1)
    }

    func testNativeReviewIsReadOnlyAndRequiresAdmissionBeforeRealCommit() throws {
        try withLedger { scope in
            let path = scope.directory.appendingPathComponent("ledger.sqlite")
            let before = try Data(contentsOf: path)
            let prepared = try processor(scope).prepare(JSONEncoder().encode(request))
            XCTAssertEqual(try Data(contentsOf: path), before)
            XCTAssertEqual(try status(scope), .notRecorded)
            var admissions = 0
            XCTAssertEqual(try prepared.confirm { admissions += 1 }, .recorded)
            XCTAssertThrowsError(try prepared.confirm { admissions += 1 })
            XCTAssertEqual(admissions, 1)
            XCTAssertEqual(try status(scope), .recorded)
        }
    }

    func testIncompleteOrChangedObservationNeverReachesAdmission() throws {
        for scenario in 0..<4 {
            try withLedger { scope in
                var current = snapshot()
                var root: String? = applications
                var processor = processor(scope, observe: { current })
                processor.userApplications = { root }
                if scenario == 0 {
                    current = snapshot(complete: false)
                    XCTAssertThrowsError(try processor.prepare(JSONEncoder().encode(request)))
                } else {
                    let review = try processor.prepare(JSONEncoder().encode(request))
                    if scenario == 1 { current = snapshot(complete: false) }
                    if scenario == 2 { current = snapshot(build: "101") }
                    if scenario == 3 { root = nil }
                    XCTAssertThrowsError(try review.confirm { XCTFail("incomplete/changed evidence admitted") })
                    current = snapshot(); root = applications
                    XCTAssertThrowsError(try review.confirm { XCTFail("failed review reused") })
                }
                XCTAssertEqual(try status(scope), .notRecorded)
            }
        }
    }

    func testRejectedAdmissionIsSingleUseAndDoesNotWrite() throws {
        try withLedger { scope in
            let review = try processor(scope).prepare(JSONEncoder().encode(request))
            XCTAssertThrowsError(try review.confirm { throw BootstrapFailure.cancelled })
            XCTAssertThrowsError(try review.confirm { XCTFail("retried rejected admission") })
            XCTAssertEqual(try status(scope), .notRecorded)
        }
    }

    func testKnownCoverageGapsOverrideCompleteAssertionBeforeOpeningStorage() throws {
        for reason in NativeCaskCoverageGap.Reason.allCases {
            let gap = NativeCaskCoverageGap(tokenPath: "/opt/homebrew/Caskroom/legacy", reason: reason)
            var storageRequests = 0
            let processor = NativeAdoptionProcessor(observe: { _ in self.snapshot(gaps: [gap]) },
                userApplications: { self.applications }, scope: { _ in
                    storageRequests += 1
                    return PrivateLedgerDirectory(home: FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString))
                })
            XCTAssertThrowsError(try processor.prepare(JSONEncoder().encode(request)))
            XCTAssertEqual(storageRequests, 0)
        }
    }

    func testNewCoverageGapConsumesReviewWithoutAdmissionOrConsent() throws {
        try withLedger { scope in
            var current = snapshot()
            let review = try processor(scope, observe: { current }).prepare(JSONEncoder().encode(request))
            current = snapshot(gaps: [NativeCaskCoverageGap(tokenPath: "/opt/homebrew/Caskroom/legacy", reason: .missingReceipt)])
            XCTAssertThrowsError(try review.confirm { XCTFail("new coverage gap admitted") })
            current = snapshot()
            XCTAssertThrowsError(try review.confirm { XCTFail("review reused after gap removed") })
            XCTAssertEqual(try status(scope), .notRecorded)
        }
    }

    func testCoverageGapAfterCommitReportsUncertaintyNotSafeRetry() throws {
        try withLedger { scope in
            var calls = 0
            let review = try processor(scope, observe: {
                calls += 1
                let gaps = calls == 5 ? [NativeCaskCoverageGap(tokenPath: "/opt/homebrew/Caskroom/legacy", reason: .missingReceipt)] : []
                return self.snapshot(gaps: gaps)
            }).prepare(JSONEncoder().encode(request))
            XCTAssertEqual(try review.confirm {}, .outcomeUnknown)
            XCTAssertEqual(try status(scope), .recorded)
            XCTAssertThrowsError(try review.confirm { XCTFail("uncertain grant replayed") })
        }
    }

    func testPostCommitObservationOrLeaseFailureReportsUncertaintyWithRealSavedConsent() throws {
        for leaseFailure in [false, true] {
            try withLedger { scope in
                var calls = 0
                let observer = {
                    calls += 1
                    if !leaseFailure && calls == 5 { throw BootstrapFailure.invalidated }
                    return self.snapshot()
                }
                let review = try processor(scope, observe: observer).prepare(JSONEncoder().encode(request))
                let lock = scope.directory.appendingPathComponent("ledger.lock")
                let moved = scope.directory.appendingPathComponent("moved.lock")
                XCTAssertEqual(try review.confirm {
                    if leaseFailure { try FileManager.default.moveItem(at: lock, to: moved) }
                }, .outcomeUnknown)
                if leaseFailure { try FileManager.default.moveItem(at: moved, to: lock) }
                XCTAssertEqual(try status(scope), .recorded)
                XCTAssertThrowsError(try review.confirm { XCTFail("uncertain commit replayed") })
            }
        }
    }

    func testAdmissionUsesSharedExclusiveBudgetAndIndependentDeadlines() throws {
        let token = Data(repeating: 1, count: 32)
        for now: UInt64 in [99, 15_000_000_100, 120_000_000_000] {
            var gate = PreflightGate(started: 0)
            gate.establish(token)
            try gate.begin(token: token, sequence: 1, bytes: 32, now: 100)
            XCTAssertThrowsError(try gate.admitAdoption(sequence: 1, now: now))
        }
        var gate = PreflightGate(started: 0)
        gate.establish(token)
        try gate.begin(token: token, sequence: 1, bytes: 32, now: 100)
        try gate.admitAdoption(sequence: 1, now: 101)
        XCTAssertThrowsError(try gate.admitAdoption(sequence: 1, now: 102))
        XCTAssertThrowsError(try gate.admitRevocation(sequence: 1, now: 102))
        XCTAssertTrue(gate.complete(sequence: 1, now: 103))
        try gate.begin(token: token, sequence: 2, bytes: 32, now: 104)
        gate.close()
        XCTAssertThrowsError(try gate.admitAdoption(sequence: 2, now: 105))
    }

    #if DEBUG
    private func connect(_ delegate: BootstrapDelegate) throws -> (NSXPCListener, ExternalUpdaterBootstrapClient) {
        let ready = expectation(description: "ready")
        let listener = NSXPCListener.anonymous(); listener.delegate = delegate; listener.activate()
        let client = try ExternalUpdaterBootstrapClient(testingConnection: NSXPCConnection(listenerEndpoint: listener.endpoint)) {
            if case .ready = $0 { ready.fulfill() }
        }
        client.begin(); wait(for: [ready], timeout: 4)
        return (listener, client)
    }

    private func review(_ client: ExternalUpdaterBootstrapClient) throws -> ExternalAdoptionReview {
        let received = expectation(description: "review")
        var review: ExternalAdoptionReview?
        client.reviewAdoption(request) { review = try? $0.get(); received.fulfill() }
        wait(for: [received], timeout: 4)
        return try XCTUnwrap(review)
    }

    func testAnonymousXPCUsesActualBridgeAndIsNotSignedPeerProof() throws {
        try withLedger { scope in
            let processor = processor(scope)
            let delegate = BootstrapDelegate(adoption: { try processor.prepare($0) })
            let (listener, client) = try connect(delegate)
            defer { client.cancel(); delegate.cancel(); listener.invalidate() }
            let prepared = try review(client)
            XCTAssertEqual(try status(scope), .notRecorded)
            let result = expectation(description: "recorded")
            client.confirmAdoption(prepared) { XCTAssertEqual(try? $0.get(), .recorded); result.fulfill() }
            wait(for: [result], timeout: 4)
            XCTAssertEqual(try status(scope), .recorded)
            let replay = expectation(description: "replay fails")
            client.confirmAdoption(prepared) { if case .success = $0 { XCTFail("replay succeeded") }; replay.fulfill() }
            wait(for: [replay], timeout: 4)
        }
    }

    func testUnconfiguredAdoptionRejectsBothMethods() throws {
        for confirmation in [false, true] {
            let delegate = BootstrapDelegate()
            let (listener, client) = try connect(delegate)
            defer { client.cancel(); delegate.cancel(); listener.invalidate() }
            let rejected = expectation(description: "not enabled")
            if confirmation {
                client.confirmAdoption(ExternalAdoptionReview(request: request, handle: Data(repeating: 1, count: 32))) {
                    if case .success = $0 { XCTFail("disabled grant succeeded") }; rejected.fulfill()
                }
            } else {
                client.reviewAdoption(request) { if case .success = $0 { XCTFail("disabled review succeeded") }; rejected.fulfill() }
            }
            wait(for: [rejected], timeout: 4)
        }
    }

    func testCancellationBeforeAndAfterAdmissionSuppressesSuccessWithoutReplay() throws {
        for before in [false, true] {
            let entered = expectation(description: "blocked")
            let finished = expectation(description: "finished")
            let cancelled = expectation(description: "cancelled")
            let released = DispatchSemaphore(value: 0)
            var writes = 0
            let delegate = BootstrapDelegate(adoption: { _ in
                PreparedAdoption { admit in
                    defer { finished.fulfill() }
                    if before { try admit() }
                    entered.fulfill()
                    _ = released.wait(timeout: .now() + 6)
                    if !before { try admit() }
                    writes += 1
                    return .recorded
                }
            }, event: { if case .closed(.cancelled) = $0 { cancelled.fulfill() } })
            let (listener, client) = try connect(delegate)
            defer { released.signal(); client.cancel(); listener.invalidate() }
            let prepared = try review(client)
            let failed = expectation(description: "failure only")
            client.confirmAdoption(prepared) { if case .success = $0 { XCTFail("lost session reported success") }; failed.fulfill() }
            wait(for: [entered], timeout: 4)
            delegate.cancel(); wait(for: [cancelled, failed], timeout: 4)
            released.signal(); wait(for: [finished], timeout: 4)
            XCTAssertEqual(writes, before ? 1 : 0)
        }
    }

    func testReviewAndConfirmShareEightRequestLimit() throws {
        var writes = 0
        let delegate = BootstrapDelegate(adoption: { _ in PreparedAdoption { admit in try admit(); writes += 1; return .recorded } })
        let (listener, client) = try connect(delegate)
        defer { client.cancel(); delegate.cancel(); listener.invalidate() }
        for _ in 0..<4 {
            let prepared = try review(client)
            let result = expectation(description: "confirmed")
            client.confirmAdoption(prepared) { XCTAssertEqual(try? $0.get(), .recorded); result.fulfill() }
            wait(for: [result], timeout: 4)
        }
        let exhausted = expectation(description: "exhausted")
        client.reviewAdoption(request) { if case .success = $0 { XCTFail("ninth accepted") }; exhausted.fulfill() }
        wait(for: [exhausted], timeout: 4)
        XCTAssertEqual(writes, 4)
    }

    func testUnauthenticatedForgedAndMalformedRequestsCannotReachPreparation() throws {
        for scenario in 0..<4 {
            let delegate = BootstrapDelegate(adoption: { _ in XCTFail("invalid request reached preparation"); throw BootstrapFailure.invalidMessage })
            let listener = NSXPCListener.anonymous(); listener.delegate = delegate; listener.activate()
            let connection = NSXPCConnection(listenerEndpoint: listener.endpoint)
            connection.remoteObjectInterface = NSXPCInterface(with: ExternalUpdaterBootstrapProtocol.self)
            connection.activate()
            defer { connection.invalidate(); delegate.cancel(); listener.invalidate() }
            let proxy = try XCTUnwrap(connection.remoteObjectProxyWithErrorHandler { _ in } as? ExternalUpdaterBootstrapProtocol)
            var token = Data(repeating: 1, count: 32)
            if scenario != 0 {
                let hello = expectation(description: "hello")
                proxy.hello(version: 1, challenge: Data(repeating: 1, count: 32)) { _, _, value in
                    if scenario != 1 { token = value ?? Data() }; hello.fulfill()
                }
                wait(for: [hello], timeout: 4)
            }
            let rejected = expectation(description: "rejected")
            let data = scenario == 2 ? Data("{}".utf8) : try JSONEncoder().encode(request)
            if scenario == 3 {
                proxy.confirmAdoption(session: token, sequence: 1, review: Data(repeating: 1, count: 32)) {
                    XCTAssertEqual($1, 0); rejected.fulfill()
                }
            } else {
                proxy.reviewAdoption(session: token, sequence: 1, request: data) {
                    XCTAssertEqual($1, 0); XCTAssertNil($2); rejected.fulfill()
                }
            }
            wait(for: [rejected], timeout: 4)
        }
    }

    func testClientRejectsForeignReplyCodesWrongEchoAndMalformedHandles() throws {
        for (confirm, code, echo, size) in [(false, UInt32(39), UInt64(1), 32), (false, 50, 2, 32),
                                           (false, 50, 1, 0), (false, 50, 1, 8193), (true, 40, 1, 32), (true, 50, 1, 32)] {
            let ready = expectation(description: "ready")
            let delegate = SilentBootstrap(); delegate.preflightResponse = (echo, code)
            delegate.reviewPayload = Data(repeating: 1, count: size)
            let listener = NSXPCListener.anonymous(); listener.delegate = delegate; listener.activate()
            let client = try ExternalUpdaterBootstrapClient(testingConnection: NSXPCConnection(listenerEndpoint: listener.endpoint)) {
                if case .ready = $0 { ready.fulfill() }
            }
            defer { client.cancel(); delegate.cancel(); listener.invalidate() }
            client.begin(); wait(for: [ready], timeout: 4)
            let rejected = expectation(description: "invalid response")
            if confirm {
                client.confirmAdoption(ExternalAdoptionReview(request: request, handle: Data(repeating: 1, count: 32))) {
                    if case .success = $0 { XCTFail("foreign outcome accepted") }; rejected.fulfill()
                }
            } else {
                client.reviewAdoption(request) { if case .success = $0 { XCTFail("invalid review accepted") }; rejected.fulfill() }
            }
            wait(for: [rejected], timeout: 4)
        }
    }
    #endif
}
