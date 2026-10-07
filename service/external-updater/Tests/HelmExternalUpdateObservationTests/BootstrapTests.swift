import Darwin
import Foundation
import XCTest
@testable import HelmExternalUpdateObservation

#if DEBUG
final class BootstrapDelegate: NSObject, NSXPCListenerDelegate {
    private let lock = NSLock()
    private var servers: [ExternalUpdaterBootstrapServer] = []
    let event: (BootstrapEvent) -> Void
    let productionGate: Bool
    let assess: ((Data) throws -> NativePolicyAssessment)?
    let consent: ((Data) throws -> ExternalConsentStatus)?
    let revocation: ((Data) throws -> PreparedRevocation)?
    let adoption: ((Data) throws -> PreparedAdoption)?
    let boundary: ((NSXPCConnection) -> NativePrivateServiceBoundary)?
    init(productionGate: Bool = false, assess: ((Data) throws -> NativePolicyAssessment)? = nil,
         consent: ((Data) throws -> ExternalConsentStatus)? = nil,
         revocation: ((Data) throws -> PreparedRevocation)? = nil,
         adoption: ((Data) throws -> PreparedAdoption)? = nil,
         boundary: ((NSXPCConnection) -> NativePrivateServiceBoundary)? = nil,
         event: @escaping (BootstrapEvent) -> Void = { _ in }) {
        self.productionGate = productionGate
        self.event = event
        self.assess = assess
        self.consent = consent
        self.revocation = revocation
        self.adoption = adoption
        self.boundary = boundary
    }
    var count: Int { lock.lock(); defer { lock.unlock() }; return servers.count }

    func listener(_ listener: NSXPCListener, shouldAcceptNewConnection connection: NSXPCConnection) -> Bool {
        let server: ExternalUpdaterBootstrapServer
        if productionGate {
            guard let guarded = try? ExternalUpdaterBootstrapServer(connection: connection, event: event) else { return false }
            server = guarded
        } else {
            server = ExternalUpdaterBootstrapServer(testingConnection: connection, assess: assess, consent: consent,
                                                   revocation: revocation, adoption: adoption,
                                                   boundary: boundary?(connection), event: event)
        }
        lock.lock(); servers.append(server); lock.unlock()
        return true
    }

    func cancel() {
        lock.lock(); let active = servers; lock.unlock()
        for server in active { server.cancel() }
    }
}

final class SilentBootstrap: NSObject, NSXPCListenerDelegate, ExternalUpdaterBootstrapProtocol {
    private let lock = NSLock()
    private var connections: [NSXPCConnection] = []
    private var held: ((UInt32, Data?, Data?) -> Void)?
    private var challenge: Data?
    var preflightResponse: (UInt64, UInt32)?
    var reviewPayload: Data? = Data(repeating: 1, count: 32)

    func listener(_ listener: NSXPCListener, shouldAcceptNewConnection connection: NSXPCConnection) -> Bool {
        connection.exportedInterface = NSXPCInterface(with: ExternalUpdaterBootstrapProtocol.self)
        connection.exportedObject = self
        lock.lock(); connections.append(connection); lock.unlock()
        connection.activate()
        return true
    }

    func hello(version: UInt32, challenge: Data, reply: @escaping (UInt32, Data?, Data?) -> Void) {
        if preflightResponse != nil {
            reply(1, challenge, Data(repeating: 7, count: 32))
            return
        }
        lock.lock(); self.challenge = challenge; held = reply; lock.unlock()
    }

    func preflight(session: Data, sequence: UInt64, request: Data, reply: @escaping (UInt64, UInt32) -> Void) {
        if let response = preflightResponse { reply(response.0, response.1); return }
        XCTFail("app data sent before authenticated readiness")
    }

    func consentStatus(session: Data, sequence: UInt64, request: Data, reply: @escaping (UInt64, UInt32) -> Void) {
        preflight(session: session, sequence: sequence, request: request, reply: reply)
    }

    func reviewRevocation(session: Data, sequence: UInt64, request: Data,
                          reply: @escaping (UInt64, UInt32, Data?) -> Void) {
        if let response = preflightResponse { reply(response.0, response.1, reviewPayload) }
    }

    func confirmRevocation(session: Data, sequence: UInt64, review: Data, reply: @escaping (UInt64, UInt32) -> Void) {
        preflight(session: session, sequence: sequence, request: review, reply: reply)
    }

    func replyLate() {
        lock.lock(); let reply = held; let echo = challenge; held = nil; lock.unlock()
        reply?(1, echo, Data(repeating: 1, count: 32))
    }

    func reviewAdoption(session: Data, sequence: UInt64, request: Data,
                        reply: @escaping (UInt64, UInt32, Data?) -> Void) {
        if let response = preflightResponse { reply(response.0, response.1, reviewPayload) }
    }

    func confirmAdoption(session: Data, sequence: UInt64, review: Data, reply: @escaping (UInt64, UInt32) -> Void) {
        preflight(session: session, sequence: sequence, request: review, reply: reply)
    }

    func cancel() {
        lock.lock(); let active = connections; lock.unlock()
        for connection in active { connection.invalidate() }
    }
}

final class BootstrapTests: XCTestCase {
    private let challenge = Data(repeating: 1, count: 32)
    private let session = Data(repeating: 2, count: 32)

    private var preflightRequest: ExternalPreflightRequest {
        ExternalPreflightRequest(targetPath: "/Applications/Example.app", bundleIdentifier: "org.example.App", installedBuild: "100")
    }

    func testConsentClientRoundTripAndWrongOperationReplyRejection() throws {
        for code in [UInt32(21), 26, 1, 6, 27] {
            let ready = expectation(description: "ready")
            let received = expectation(description: "consent reply")
            let delegate = SilentBootstrap()
            delegate.preflightResponse = (1, code)
            let listener = NSXPCListener.anonymous()
            listener.delegate = delegate
            listener.activate()
            let client = try ExternalUpdaterBootstrapClient(testingConnection: NSXPCConnection(listenerEndpoint: listener.endpoint)) {
                if case .ready = $0 { ready.fulfill() }
            }
            defer { client.cancel(); delegate.cancel(); listener.invalidate() }
            client.begin()
            wait(for: [ready], timeout: 4)
            client.consentStatus(preflightRequest) { result in
                if code == 21 {
                    XCTAssertEqual(try? result.get(), .recorded)
                } else if code == 26 {
                    XCTAssertEqual(try? result.get(), .scopeChanged)
                } else if case .success = result { XCTFail("foreign result accepted as consent") }
                received.fulfill()
            }
            wait(for: [received], timeout: 4)
        }
    }

    func testConsentAndPreflightShareOneSequenceAndRequestBudget() throws {
        let delegate = BootstrapDelegate(assess: { _ in .unresolved }, consent: { _ in .notRecorded })
        let listener = NSXPCListener.anonymous()
        listener.delegate = delegate
        listener.activate()
        let connection = NSXPCConnection(listenerEndpoint: listener.endpoint)
        connection.remoteObjectInterface = NSXPCInterface(with: ExternalUpdaterBootstrapProtocol.self)
        connection.activate()
        defer { connection.invalidate(); delegate.cancel(); listener.invalidate() }
        let proxy = try XCTUnwrap(connection.remoteObjectProxyWithErrorHandler { _ in } as? ExternalUpdaterBootstrapProtocol)
        let hello = expectation(description: "hello")
        var token = Data()
        proxy.hello(version: 1, challenge: challenge) { _, _, session in token = session ?? Data(); hello.fulfill() }
        wait(for: [hello], timeout: 4)
        let data = try JSONEncoder().encode(preflightRequest)
        for sequence in UInt64(1)...9 {
            let replied = expectation(description: "sequence \(sequence)")
            let callback: (UInt64, UInt32) -> Void = { echo, code in
                XCTAssertEqual(echo, sequence)
                XCTAssertEqual(code, sequence == 9 ? 0 : sequence.isMultiple(of: 2) ? 1 : 20)
                replied.fulfill()
            }
            if sequence.isMultiple(of: 2) {
                proxy.preflight(session: token, sequence: sequence, request: data, reply: callback)
            } else {
                proxy.consentStatus(session: token, sequence: sequence, request: data, reply: callback)
            }
            wait(for: [replied], timeout: 4)
        }
    }

    func testConsentCancellationDiscardsLateHistoryResult() throws {
        let ready = expectation(description: "ready")
        let entered = expectation(description: "inspection entered")
        let cancelled = expectation(description: "cancelled callback")
        let release = DispatchSemaphore(value: 0)
        let delegate = BootstrapDelegate(consent: { _ in
            entered.fulfill()
            _ = release.wait(timeout: .now() + 5)
            return .recorded
        })
        let listener = NSXPCListener.anonymous()
        listener.delegate = delegate
        listener.activate()
        let client = try ExternalUpdaterBootstrapClient(testingConnection: NSXPCConnection(listenerEndpoint: listener.endpoint)) {
            if case .ready = $0 { ready.fulfill() }
        }
        defer { release.signal(); client.cancel(); delegate.cancel(); listener.invalidate() }
        client.begin()
        wait(for: [ready], timeout: 4)
        client.consentStatus(preflightRequest) { result in
            if case .success = result { XCTFail("cancelled history published") }
            cancelled.fulfill()
        }
        wait(for: [entered], timeout: 4)
        client.cancel()
        wait(for: [cancelled], timeout: 4)
        release.signal()
    }

    func testClientRejectsWrongSequenceAndUnknownOrFailedResponseCodes() throws {
        for response in [(UInt64(2), UInt32(1)), (1, 99), (1, 0), (1, 6)] {
            let ready = expectation(description: "ready")
            let failed = expectation(description: "invalid result rejected once")
            let server = SilentBootstrap()
            server.preflightResponse = response
            let listener = NSXPCListener.anonymous()
            listener.delegate = server
            listener.activate()
            let client = try ExternalUpdaterBootstrapClient(testingConnection: NSXPCConnection(listenerEndpoint: listener.endpoint)) {
                if case .ready = $0 { ready.fulfill() }
            }
            defer { client.cancel(); server.cancel(); listener.invalidate() }
            client.begin()
            wait(for: [ready], timeout: 3)
            client.preflight(preflightRequest) {
                if case .failure(.invalidMessage) = $0 { failed.fulfill() } else { XCTFail("bad response became a result") }
            }
            wait(for: [failed], timeout: 3)
        }
    }

    func testPreflightUsesRealXPCOnlyAfterReadyAndKeepsPolicyUnresolved() throws {
        let ready = expectation(description: "authenticated test transport ready")
        let checked = expectation(description: "strict request reached evaluator")
        let result = expectation(description: "unresolved response")
        let delegate = BootstrapDelegate(assess: { bytes in
            let request = try ExternalPreflightRequest.decode(bytes, userApplications: nil)
            XCTAssertEqual(request.expectedInstalledBuild, "100")
            checked.fulfill()
            return .unresolved
        })
        let listener = NSXPCListener.anonymous()
        listener.delegate = delegate
        listener.activate()
        let client = try ExternalUpdaterBootstrapClient(testingConnection: NSXPCConnection(listenerEndpoint: listener.endpoint)) {
            if case .ready = $0 { ready.fulfill() }
        }
        defer { client.cancel(); delegate.cancel(); listener.invalidate() }
        let rejected = expectation(description: "no app data before readiness")
        client.preflight(preflightRequest) {
            if case .failure(.invalidMessage) = $0 { rejected.fulfill() } else { XCTFail("request sent before hello") }
        }
        wait(for: [rejected], timeout: 2)
        XCTAssertEqual(delegate.count, 0)
        client.begin()
        wait(for: [ready], timeout: 3)
        client.preflight(preflightRequest) {
            if case .success(.unresolved) = $0 { result.fulfill() } else { XCTFail("unexpected preflight outcome: \($0)") }
        }
        wait(for: [checked, result], timeout: 3)
    }

    func testForgedSessionAndPreHelloRequestsCannotReachObservation() throws {
        for scenario in [(false, false), (true, false), (false, true), (true, true)] {
            let (helloFirst, consent) = scenario
            let closed = expectation(description: "invalid app request closes server")
            let delegate = BootstrapDelegate(assess: { _ in XCTFail("unauthorized observation"); return .unresolved },
                                             consent: { _ in XCTFail("unauthorized history read"); return .notRecorded }, event: {
                if case .closed = $0 { closed.fulfill() }
            })
            let listener = NSXPCListener.anonymous()
            listener.delegate = delegate
            listener.activate()
            let connection = NSXPCConnection(listenerEndpoint: listener.endpoint)
            connection.remoteObjectInterface = NSXPCInterface(with: ExternalUpdaterBootstrapProtocol.self)
            connection.activate()
            defer { connection.invalidate(); delegate.cancel(); listener.invalidate() }
            let proxy = try XCTUnwrap(connection.remoteObjectProxyWithErrorHandler { _ in } as? ExternalUpdaterBootstrapProtocol)
            let request = try JSONEncoder().encode(preflightRequest)
            let send = {
                if consent {
                    proxy.consentStatus(session: self.session, sequence: 1, request: request) { _, code in XCTAssertEqual(code, 0) }
                } else {
                    proxy.preflight(session: self.session, sequence: 1, request: request) { _, code in XCTAssertEqual(code, 0) }
                }
            }
            if helloFirst { proxy.hello(version: 1, challenge: challenge) { _, _, _ in send() } } else { send() }
            wait(for: [closed], timeout: 3)
        }
    }

    func testReplayAndConcurrentRequestsInvalidateSession() throws {
        for concurrent in [false, true] {
            let closed = expectation(description: "replay rejected")
            let entered = expectation(description: "one observation")
            let release = DispatchSemaphore(value: 0)
            let delegate = BootstrapDelegate(assess: { _ in
                entered.fulfill()
                if concurrent { _ = release.wait(timeout: .now() + .seconds(5)) }
                return .unresolved
            }, event: { if case .closed = $0 { closed.fulfill() } })
            let listener = NSXPCListener.anonymous()
            listener.delegate = delegate
            listener.activate()
            let connection = NSXPCConnection(listenerEndpoint: listener.endpoint)
            connection.remoteObjectInterface = NSXPCInterface(with: ExternalUpdaterBootstrapProtocol.self)
            connection.activate()
            defer { release.signal(); connection.invalidate(); delegate.cancel(); listener.invalidate() }
            let proxy = try XCTUnwrap(connection.remoteObjectProxyWithErrorHandler { _ in } as? ExternalUpdaterBootstrapProtocol)
            let bytes = try JSONEncoder().encode(preflightRequest)
            proxy.hello(version: 1, challenge: challenge) { _, _, token in
                guard let token else { XCTFail("no session"); return }
                proxy.preflight(session: token, sequence: 1, request: bytes) { _, code in
                    if !concurrent {
                        XCTAssertEqual(code, 1)
                        proxy.preflight(session: token, sequence: 1, request: bytes) { _, _ in }
                    }
                }
                if concurrent { proxy.preflight(session: token, sequence: 2, request: bytes) { _, _ in } }
            }
            wait(for: [entered, closed], timeout: 3)
        }
    }

    func testCancellationDuringInspectionDiscardsLateResult() throws {
        let ready = expectation(description: "ready")
        let entered = expectation(description: "native read in progress")
        let cancelled = expectation(description: "pending callback fails exactly once")
        let release = DispatchSemaphore(value: 0)
        let finished = expectation(description: "worker returns after cancellation")
        let delegate = BootstrapDelegate(assess: { _ in
            entered.fulfill()
            _ = release.wait(timeout: .now() + .seconds(5))
            finished.fulfill()
            return .unresolved
        })
        let listener = NSXPCListener.anonymous()
        listener.delegate = delegate
        listener.activate()
        let client = try ExternalUpdaterBootstrapClient(testingConnection: NSXPCConnection(listenerEndpoint: listener.endpoint)) {
            if case .ready = $0 { ready.fulfill() }
        }
        defer { release.signal(); client.cancel(); delegate.cancel(); listener.invalidate() }
        client.begin()
        wait(for: [ready], timeout: 3)
        client.preflight(preflightRequest) {
            if case .failure = $0 { cancelled.fulfill() } else { XCTFail("late observation published") }
        }
        wait(for: [entered], timeout: 3)
        client.cancel()
        wait(for: [cancelled], timeout: 2)
        release.signal()
        wait(for: [finished], timeout: 2)
        let drained = expectation(description: "late reply drain")
        DispatchQueue.global().asyncAfter(deadline: .now() + 0.1) { drained.fulfill() }
        wait(for: [drained], timeout: 1)
    }

    func testReplyGateRequiresExactChallengeVersionAccountAndBoundedSession() throws {
        var valid = BootstrapReplyGate(challenge: challenge, account: 501, started: 100)
        try valid.accept(version: 1, echo: challenge, session: session, peerAccount: 501, now: 101)
        XCTAssertTrue(valid.established)
        XCTAssertThrowsError(try valid.accept(version: 1, echo: challenge, session: session, peerAccount: 501, now: 102))
        for (version, echo, token, account) in [
            (UInt32(2), challenge, session, uid_t(501)),
            (1, Data(repeating: 3, count: 32), session, 501),
            (1, challenge, Data(), 501),
            (1, challenge, Data(repeating: 2, count: 33), 501),
            (1, challenge, session, 502),
            (1, challenge, session, 0)
        ] {
            var gate = BootstrapReplyGate(challenge: challenge, account: 501, started: 100)
            XCTAssertThrowsError(try gate.accept(version: version, echo: echo, session: token, peerAccount: account, now: 101))
            XCTAssertFalse(gate.established)
        }
        var missing = BootstrapReplyGate(challenge: challenge, account: 501, started: 100)
        XCTAssertThrowsError(try missing.accept(version: 1, echo: nil, session: nil, peerAccount: 501, now: 101))
    }

    func testReplyGateRejectsExpiredBackwardsAndClosedSessions() {
        for now in [UInt64(99), 100 + BootstrapWire.handshakeNanoseconds, UInt64.max] {
            var gate = BootstrapReplyGate(challenge: challenge, account: 501, started: 100)
            XCTAssertThrowsError(try gate.accept(version: 1, echo: challenge, session: session, peerAccount: 501, now: now)) {
                XCTAssertEqual($0 as? BootstrapFailure, .expired)
            }
        }
        var gate = BootstrapReplyGate(challenge: challenge, account: 501, started: 100)
        XCTAssertTrue(gate.close())
        XCTAssertFalse(gate.close())
        XCTAssertThrowsError(try gate.accept(version: 1, echo: challenge, session: session, peerAccount: 501, now: 101))
        XCTAssertFalse(gate.established)
    }

    func testNonceHasRequiredShapeAndIndependentSamples() throws {
        let first = try BootstrapWire.nonce()
        XCTAssertEqual(first.count, 32)
        XCTAssertNotEqual(first, try BootstrapWire.nonce())
    }

    func testNativeHandshakeCompletesOnceAndClosesOnServerLoss() throws {
        let ready = expectation(description: "client ready once")
        let closed = expectation(description: "client closed once")
        let serverReady = expectation(description: "server ready once")
        let delegate = BootstrapDelegate(event: { if case .ready = $0 { serverReady.fulfill() } })
        let listener = NSXPCListener.anonymous()
        listener.delegate = delegate
        listener.activate()
        let connection = NSXPCConnection(listenerEndpoint: listener.endpoint)
        let client = try ExternalUpdaterBootstrapClient(testingConnection: connection) {
            switch $0 {
            case .ready: ready.fulfill()
            case .closed: closed.fulfill()
            }
        }
        defer { client.cancel(); delegate.cancel(); listener.invalidate() }
        client.begin()
        client.begin()
        wait(for: [ready, serverReady], timeout: 4)
        XCTAssertEqual(connection.effectiveUserIdentifier, geteuid())
        XCTAssertEqual(delegate.count, 1)
        delegate.cancel()
        wait(for: [closed], timeout: 4)
        client.begin()
        XCTAssertEqual(delegate.count, 1)
    }

    func testStrictClientRejectsUnsignedHelperInsteadOfBecomingReady() throws {
        let closed = expectation(description: "impostor rejected")
        let delegate = BootstrapDelegate()
        let listener = NSXPCListener.anonymous()
        listener.delegate = delegate
        listener.activate()
        let client = try ExternalUpdaterBootstrapClient(endpoint: listener.endpoint) {
            switch $0 {
            case .ready: XCTFail("unsigned helper became authenticated")
            case .closed: closed.fulfill()
            }
        }
        defer { client.cancel(); delegate.cancel(); listener.invalidate() }
        client.begin()
        wait(for: [closed], timeout: 4)
    }

    func testCancellationBeforeBeginDoesNotContactEndpoint() throws {
        let closed = expectation(description: "cancelled once")
        let delegate = BootstrapDelegate()
        let listener = NSXPCListener.anonymous()
        listener.delegate = delegate
        listener.activate()
        let connection = NSXPCConnection(listenerEndpoint: listener.endpoint)
        let client = try ExternalUpdaterBootstrapClient(testingConnection: connection) {
            switch $0 {
            case .ready: XCTFail("cancelled session became ready")
            case .closed(let reason): XCTAssertEqual(reason, .cancelled); closed.fulfill()
            }
        }
        defer { delegate.cancel(); listener.invalidate() }
        client.cancel()
        client.begin()
        client.cancel()
        wait(for: [closed], timeout: 4)
        XCTAssertEqual(delegate.count, 0)
    }

    func testMissingReplyTimesOutAndLateReplyCannotBecomeReady() throws {
        let closed = expectation(description: "handshake deadline")
        let delegate = SilentBootstrap()
        let listener = NSXPCListener.anonymous()
        listener.delegate = delegate
        listener.activate()
        let connection = NSXPCConnection(listenerEndpoint: listener.endpoint)
        let client = try ExternalUpdaterBootstrapClient(testingConnection: connection) {
            switch $0 {
            case .ready: XCTFail("late reply became ready")
            case .closed(let reason): XCTAssertEqual(reason, .expired); closed.fulfill()
            }
        }
        defer { client.cancel(); delegate.cancel(); listener.invalidate() }
        client.begin()
        wait(for: [closed], timeout: 8)
        delegate.replyLate()
        let drained = expectation(description: "late callback drain")
        DispatchQueue.global().asyncAfter(deadline: .now() + 0.1) { drained.fulfill() }
        wait(for: [drained], timeout: 2)
    }

    func testServerRejectsInvalidVersionAndNonceBeforeReady() throws {
        for (version, nonce) in [(UInt32(2), challenge), (1, Data(repeating: 1, count: 33))] {
            let closed = expectation(description: "invalid hello closes server")
            let delegate = BootstrapDelegate(event: {
                switch $0 {
                case .ready: XCTFail("malformed hello accepted")
                case .closed(let reason): XCTAssertEqual(reason, .invalidMessage); closed.fulfill()
                }
            })
            let listener = NSXPCListener.anonymous()
            listener.delegate = delegate
            listener.activate()
            let connection = NSXPCConnection(listenerEndpoint: listener.endpoint)
            connection.remoteObjectInterface = NSXPCInterface(with: ExternalUpdaterBootstrapProtocol.self)
            connection.activate()
            defer { connection.invalidate(); delegate.cancel(); listener.invalidate() }
            let proxy = try XCTUnwrap(connection.remoteObjectProxyWithErrorHandler { _ in } as? ExternalUpdaterBootstrapProtocol)
            proxy.hello(version: version, challenge: nonce) { version, echo, session in
                XCTAssertEqual(version, 0)
                XCTAssertNil(echo)
                XCTAssertNil(session)
            }
            wait(for: [closed], timeout: 4)
        }
    }

    func testProductionServerRejectsUnsignedClientBeforeHello() throws {
        let closed = expectation(description: "server identity gate rejects unsigned client")
        let delegate = BootstrapDelegate(productionGate: true, event: {
            if case .ready = $0 { XCTFail("unsigned caller passed production server gate") }
        })
        let listener = NSXPCListener.anonymous()
        listener.delegate = delegate
        listener.activate()
        let connection = NSXPCConnection(listenerEndpoint: listener.endpoint)
        let client = try ExternalUpdaterBootstrapClient(testingConnection: connection) {
            switch $0 {
            case .ready: XCTFail("unsigned client completed production handshake")
            case .closed: closed.fulfill()
            }
        }
        defer { client.cancel(); delegate.cancel(); listener.invalidate() }
        client.begin()
        wait(for: [closed], timeout: 4)
    }

    func testServerAcceptsOnlyOneHelloPerConnection() throws {
        let ready = expectation(description: "one server handshake")
        ready.assertForOverFulfill = true
        let closed = expectation(description: "replayed hello closes session")
        let delegate = BootstrapDelegate(event: {
            switch $0 {
            case .ready: ready.fulfill()
            case .closed(let reason): XCTAssertEqual(reason, .invalidMessage); closed.fulfill()
            }
        })
        let listener = NSXPCListener.anonymous()
        listener.delegate = delegate
        listener.activate()
        let connection = NSXPCConnection(listenerEndpoint: listener.endpoint)
        connection.remoteObjectInterface = NSXPCInterface(with: ExternalUpdaterBootstrapProtocol.self)
        connection.activate()
        defer { connection.invalidate(); delegate.cancel(); listener.invalidate() }
        let proxy = try XCTUnwrap(connection.remoteObjectProxyWithErrorHandler { _ in } as? ExternalUpdaterBootstrapProtocol)
        proxy.hello(version: 1, challenge: challenge) { version, echo, session in
            XCTAssertEqual(version, 1)
            XCTAssertEqual(echo, self.challenge)
            XCTAssertEqual(session?.count, 32)
            proxy.hello(version: 1, challenge: self.challenge) { version, _, _ in XCTAssertEqual(version, 0) }
        }
        wait(for: [ready, closed], timeout: 4)
    }

    func testBlockedConsumerCannotPreventConnectionCancellation() throws {
        let entered = expectation(description: "consumer entered")
        let invalidated = expectation(description: "native connection cancelled while consumer is blocked")
        let released = DispatchSemaphore(value: 0)
        let delegate = BootstrapDelegate()
        let listener = NSXPCListener.anonymous()
        listener.delegate = delegate
        listener.activate()
        let connection = NSXPCConnection(listenerEndpoint: listener.endpoint)
        let client = try ExternalUpdaterBootstrapClient(testingConnection: connection) {
            if case .ready = $0 {
                entered.fulfill()
                _ = released.wait(timeout: .now() + .seconds(5))
            }
        }
        // Observe native invalidation instead of waiting for the deliberately
        // blocked consumer queue. Explicit cancellation still drives client state.
        connection.invalidationHandler = { invalidated.fulfill() }
        defer { released.signal(); client.cancel(); delegate.cancel(); listener.invalidate() }
        client.begin()
        wait(for: [entered], timeout: 3)
        client.cancel()
        wait(for: [invalidated], timeout: 2)
    }
}
#endif
