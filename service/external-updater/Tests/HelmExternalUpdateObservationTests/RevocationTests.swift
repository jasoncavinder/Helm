import Foundation
import XCTest
@testable import HelmExternalUpdateObservation

final class RevocationTests: XCTestCase {
    func testStrictRequestAcceptsMissingAppButNoAuthorityOrDatabaseClaims() throws {
        let request = ExternalRevocationRequest(targetPath: "/Applications/Gone.app")
        let data = try JSONEncoder().encode(request)
        XCTAssertNoThrow(try ExternalRevocationRequest.decode(data, root: nil))
        for path in ["/tmp/Gone.app", "/Applications/../Gone.app", "/Applications/Host.app/Nested.app"] {
            XCTAssertThrowsError(try ExternalRevocationRequest.decode(JSONEncoder().encode(ExternalRevocationRequest(targetPath: path)), root: nil))
        }
        for field in ["authority", "revision", "epoch", "databasePath", "confirmed"] {
            var object = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
            object[field] = true
            XCTAssertThrowsError(try ExternalRevocationRequest.decode(JSONSerialization.data(withJSONObject: object), root: nil))
        }
    }

    func testNativeReviewReadOnlySingleUseAndStaleRevision() throws {
        let root = FileManager.default.homeDirectoryForCurrentUser.resolvingSymlinksInPath().appendingPathComponent("helm-revocation-test-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false, attributes: [.posixPermissions: 0o700])
        defer { try? FileManager.default.removeItem(at: root) }
        let scope = PrivateLedgerDirectory(home: root)
        try scope.withDatabase { try NativeHelperLedger.initialize(path: $0, fresh: $1) }
        let database = scope.directory.appendingPathComponent("ledger.sqlite")
        let before = try Data(contentsOf: database)
        let request = try JSONEncoder().encode(ExternalRevocationRequest(targetPath: "/Applications/Gone.app"))
        let first = try scope.withDatabase(createIfMissing: false) { path, _ in
            try NativeRevocationReview(path: path, request: request, root: nil, now: 100)
        }
        let second = try scope.withDatabase(createIfMissing: false) { path, _ in
            try NativeRevocationReview(path: path, request: request, root: nil, now: 100)
        }
        XCTAssertEqual(try Data(contentsOf: database), before)
        try scope.withDatabase(createIfMissing: false) { path, _ in
            XCTAssertEqual(first.confirm(path: path, now: 110), .revoked)
            XCTAssertEqual(first.confirm(path: path, now: 110), .reviewChanged)
            XCTAssertEqual(second.confirm(path: path, now: 110), .reviewChanged)
        }
    }

    func testHandlesAreConnectionLocalReplaceableSingleUseAndCleared() throws {
        var writes = 0
        let prepare: (Data) throws -> PreparedRevocation = { _ in
            PreparedRevocation { admit in try admit(); writes += 1; return .revoked }
        }
        let first = RevocationCoordinator(prepare: prepare)
        let other = RevocationCoordinator(prepare: prepare)
        let old = try first.review(Data([1]))
        let fresh = try first.review(Data([2]))
        XCTAssertNotEqual(old, fresh)
        XCTAssertThrowsError(try other.confirm(fresh) {})
        XCTAssertThrowsError(try first.confirm(old) {})
        XCTAssertThrowsError(try first.confirm(fresh) {}) // invalid attempt consumed pending review
        let current = try first.review(Data([3]))
        XCTAssertEqual(try first.confirm(current) {}, .revoked)
        XCTAssertThrowsError(try first.confirm(current) {})
        let abandoned = try first.review(Data([4]))
        first.clear()
        XCTAssertThrowsError(try first.confirm(abandoned) {})
        XCTAssertEqual(writes, 1)
    }

    func testAdmissionCannotBeReusedOrArriveAfterCancellationOrDeadline() throws {
        let token = Data(repeating: 1, count: 32)
        for now: UInt64 in [99, 15_000_000_100, 120_000_000_000] {
            var gate = PreflightGate(started: 0)
            gate.establish(token)
            try gate.begin(token: token, sequence: 1, bytes: 32, now: 100)
            XCTAssertThrowsError(try gate.admitRevocation(sequence: 1, now: now))
        }
        var gate = PreflightGate(started: 0)
        gate.establish(token)
        try gate.begin(token: token, sequence: 1, bytes: 32, now: 100)
        try gate.admitRevocation(sequence: 1, now: 101)
        XCTAssertThrowsError(try gate.admitRevocation(sequence: 1, now: 102))
        XCTAssertTrue(gate.complete(sequence: 1, now: 103))
        try gate.begin(token: token, sequence: 2, bytes: 32, now: 104)
        gate.close()
        XCTAssertThrowsError(try gate.admitRevocation(sequence: 2, now: 105))
    }

    #if DEBUG
    private func connect(_ delegate: BootstrapDelegate) throws -> (NSXPCListener, ExternalUpdaterBootstrapClient) {
        let ready = expectation(description: "ready")
        let listener = NSXPCListener.anonymous()
        listener.delegate = delegate
        listener.activate()
        let client = try ExternalUpdaterBootstrapClient(testingConnection: NSXPCConnection(listenerEndpoint: listener.endpoint)) {
            if case .ready = $0 { ready.fulfill() }
        }
        client.begin()
        wait(for: [ready], timeout: 4)
        return (listener, client)
    }

    private func review(_ client: ExternalUpdaterBootstrapClient) throws -> ExternalRevocationReview {
        let received = expectation(description: "review")
        var value: ExternalRevocationReview?
        client.reviewRevocation(ExternalRevocationRequest(targetPath: "/Applications/Gone.app")) {
            value = try? $0.get(); received.fulfill()
        }
        wait(for: [received], timeout: 4)
        return try XCTUnwrap(value)
    }

    func testAnonymousXPCReviewConfirmAndReplayShareBudget() throws {
        var writes = 0
        let delegate = BootstrapDelegate(revocation: { _ in
            PreparedRevocation { admit in try admit(); writes += 1; return .revoked }
        })
        let (listener, client) = try connect(delegate)
        defer { client.cancel(); delegate.cancel(); listener.invalidate() }
        for _ in 0..<4 {
            let prepared = try review(client)
            let finished = expectation(description: "confirmed")
            client.confirmRevocation(prepared) { XCTAssertEqual(try? $0.get(), .revoked); finished.fulfill() }
            wait(for: [finished], timeout: 4)
        }
        let exhausted = expectation(description: "budget exhausted")
        client.reviewRevocation(ExternalRevocationRequest(targetPath: "/Applications/Gone.app")) {
            if case .success = $0 { XCTFail("ninth request accepted") }; exhausted.fulfill()
        }
        wait(for: [exhausted], timeout: 4)
        XCTAssertEqual(writes, 4)
    }

    func testCancellationBeforeAdmissionCannotWriteAndAfterAdmissionCannotReportSuccess() throws {
        for admitBeforeCancel in [false, true] {
            let entered = expectation(description: "worker blocked")
            let completed = expectation(description: "worker finished")
            let cancelled = expectation(description: "server cancelled")
            let released = DispatchSemaphore(value: 0)
            var writes = 0
            let delegate = BootstrapDelegate(revocation: { _ in
                PreparedRevocation { admit in
                    defer { completed.fulfill() }
                    if admitBeforeCancel { try admit() }
                    entered.fulfill()
                    _ = released.wait(timeout: .now() + 6)
                    if !admitBeforeCancel { try admit() }
                    writes += 1
                    return .revoked
                }
            }, event: { if case .closed(.cancelled) = $0 { cancelled.fulfill() } })
            let (listener, client) = try connect(delegate)
            defer { released.signal(); client.cancel(); listener.invalidate() }
            let prepared = try review(client)
            let result = expectation(description: "failed response")
            client.confirmRevocation(prepared) {
                if case .success = $0 { XCTFail("lost session published success") }; result.fulfill()
            }
            wait(for: [entered], timeout: 4)
            delegate.cancel()
            wait(for: [cancelled, result], timeout: 4)
            released.signal()
            wait(for: [completed], timeout: 4)
            XCTAssertEqual(writes, admitBeforeCancel ? 1 : 0)
        }
    }

    func testClientRejectsWrongReviewCodesSequenceAndPayloadLength() throws {
        for (echo, code, bytes) in [(UInt64(1), UInt32(21), 32), (2, 39, 32), (1, 39, 0), (1, 39, 8193)] {
            let ready = expectation(description: "ready")
            let result = expectation(description: "bad response")
            let delegate = SilentBootstrap()
            delegate.preflightResponse = (echo, code)
            delegate.reviewPayload = Data(repeating: 1, count: bytes)
            let listener = NSXPCListener.anonymous(); listener.delegate = delegate; listener.activate()
            let client = try ExternalUpdaterBootstrapClient(testingConnection: NSXPCConnection(listenerEndpoint: listener.endpoint)) {
                if case .ready = $0 { ready.fulfill() }
            }
            defer { client.cancel(); delegate.cancel(); listener.invalidate() }
            client.begin(); wait(for: [ready], timeout: 4)
            client.reviewRevocation(ExternalRevocationRequest(targetPath: "/Applications/Gone.app")) {
                if case .success = $0 { XCTFail("foreign/malformed reply accepted") }; result.fulfill()
            }
            wait(for: [result], timeout: 4)
        }
    }

    func testServerRejectsRevocationBeforeHelloAndForgedSession() throws {
        for handshake in [false, true] {
            let delegate = BootstrapDelegate(revocation: { _ in XCTFail("unauthenticated review entered"); throw BootstrapFailure.invalidMessage })
            let listener = NSXPCListener.anonymous(); listener.delegate = delegate; listener.activate()
            let connection = NSXPCConnection(listenerEndpoint: listener.endpoint)
            connection.remoteObjectInterface = NSXPCInterface(with: ExternalUpdaterBootstrapProtocol.self)
            connection.activate()
            defer { connection.invalidate(); delegate.cancel(); listener.invalidate() }
            let proxy = try XCTUnwrap(connection.remoteObjectProxyWithErrorHandler { _ in } as? ExternalUpdaterBootstrapProtocol)
            if handshake {
                let hello = expectation(description: "hello")
                proxy.hello(version: 1, challenge: Data(repeating: 1, count: 32)) { _, _, _ in hello.fulfill() }
                wait(for: [hello], timeout: 4)
            }
            let failed = expectation(description: "forged request")
            proxy.reviewRevocation(session: Data(repeating: 0, count: 32), sequence: 1,
                                   request: try JSONEncoder().encode(ExternalRevocationRequest(targetPath: "/Applications/Gone.app"))) {
                XCTAssertEqual($0, 1); XCTAssertEqual($1, 0); XCTAssertNil($2); failed.fulfill()
            }
            wait(for: [failed], timeout: 4)
        }
    }

    func testClientOnlyAcceptsRevocationOutcomeCodesForConfirmation() throws {
        for code: UInt32 in [40, 41, 42, 21, 39] {
            let delegate = SilentBootstrap(); delegate.preflightResponse = (1, code)
            let ready = expectation(description: "ready")
            let listener = NSXPCListener.anonymous(); listener.delegate = delegate; listener.activate()
            let client = try ExternalUpdaterBootstrapClient(testingConnection: NSXPCConnection(listenerEndpoint: listener.endpoint)) {
                if case .ready = $0 { ready.fulfill() }
            }
            defer { client.cancel(); delegate.cancel(); listener.invalidate() }
            client.begin(); wait(for: [ready], timeout: 4)
            let result = expectation(description: "result")
            client.confirmRevocation(ExternalRevocationReview(targetPath: "/Applications/Gone.app", handle: Data(repeating: 1, count: 32))) {
                if let expected = ExternalRevocationOutcome(code: code) { XCTAssertEqual(try? $0.get(), expected) } else if case .success = $0 {
                    XCTFail("foreign code accepted")
                }
                result.fulfill()
            }
            wait(for: [result], timeout: 4)
        }
    }
    #endif
}
