import Darwin
import Foundation
import XCTest
@testable import HelmExternalUpdateObservation

#if DEBUG
private final class BootstrapDelegate: NSObject, NSXPCListenerDelegate {
    private let lock = NSLock()
    private var servers: [ExternalUpdaterBootstrapServer] = []
    let event: (BootstrapEvent) -> Void
    let productionGate: Bool
    init(productionGate: Bool = false, event: @escaping (BootstrapEvent) -> Void = { _ in }) {
        self.productionGate = productionGate
        self.event = event
    }
    var count: Int { lock.lock(); defer { lock.unlock() }; return servers.count }

    func listener(_ listener: NSXPCListener, shouldAcceptNewConnection connection: NSXPCConnection) -> Bool {
        let server: ExternalUpdaterBootstrapServer
        if productionGate {
            guard let guarded = try? ExternalUpdaterBootstrapServer(connection: connection, event: event) else { return false }
            server = guarded
        } else {
            server = ExternalUpdaterBootstrapServer(testingConnection: connection, event: event)
        }
        lock.lock(); servers.append(server); lock.unlock()
        return true
    }

    func cancel() {
        lock.lock(); let active = servers; lock.unlock()
        for server in active { server.cancel() }
    }
}

private final class SilentBootstrap: NSObject, NSXPCListenerDelegate, ExternalUpdaterBootstrapProtocol {
    private let lock = NSLock()
    private var connections: [NSXPCConnection] = []
    private var held: ((UInt32, Data?, Data?) -> Void)?
    private var challenge: Data?

    func listener(_ listener: NSXPCListener, shouldAcceptNewConnection connection: NSXPCConnection) -> Bool {
        connection.exportedInterface = NSXPCInterface(with: ExternalUpdaterBootstrapProtocol.self)
        connection.exportedObject = self
        lock.lock(); connections.append(connection); lock.unlock()
        connection.activate()
        return true
    }

    func hello(version: UInt32, challenge: Data, reply: @escaping (UInt32, Data?, Data?) -> Void) {
        lock.lock(); self.challenge = challenge; held = reply; lock.unlock()
    }

    func replyLate() {
        lock.lock(); let reply = held; let echo = challenge; held = nil; lock.unlock()
        reply?(1, echo, Data(repeating: 1, count: 32))
    }

    func cancel() {
        lock.lock(); let active = connections; lock.unlock()
        for connection in active { connection.invalidate() }
    }
}

final class BootstrapTests: XCTestCase {
    private let challenge = Data(repeating: 1, count: 32)
    private let session = Data(repeating: 2, count: 32)

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
        let delegate = BootstrapDelegate { if case .ready = $0 { serverReady.fulfill() } }
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
            let delegate = BootstrapDelegate {
                switch $0 {
                case .ready: XCTFail("malformed hello accepted")
                case .closed(let reason): XCTAssertEqual(reason, .invalidMessage); closed.fulfill()
                }
            }
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
        let delegate = BootstrapDelegate(productionGate: true) {
            if case .ready = $0 { XCTFail("unsigned caller passed production server gate") }
        }
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
        let delegate = BootstrapDelegate {
            switch $0 {
            case .ready: ready.fulfill()
            case .closed(let reason): XCTAssertEqual(reason, .invalidMessage); closed.fulfill()
            }
        }
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
