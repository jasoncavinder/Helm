import Foundation
import Security
import XCTest
@testable import HelmExternalUpdateObservation

@objc private protocol ProbeProtocol {
    func ping(reply: @escaping (String) -> Void)
}

private final class ProbeService: NSObject, ProbeProtocol {
    private let lock = NSLock()
    private var calls = 0
    var count: Int { lock.lock(); defer { lock.unlock() }; return calls }
    func ping(reply: @escaping (String) -> Void) {
        lock.lock(); calls += 1; lock.unlock()
        reply("pong")
    }
}

private final class ProbeDelegate: NSObject, NSXPCListenerDelegate {
    let service = ProbeService()
    private let lock = NSLock()
    private var connections: [NSXPCConnection] = []
    private(set) var authentication: ExternalUpdaterPeerAuthentication?
    var count: Int { lock.lock(); defer { lock.unlock() }; return connections.count }

    init(authentication: ExternalUpdaterPeerAuthentication? = nil) { self.authentication = authentication }

    func listener(_ listener: NSXPCListener, shouldAcceptNewConnection connection: NSXPCConnection) -> Bool {
        if let authentication, !authentication.admit(connection) { return false }
        connection.exportedInterface = NSXPCInterface(with: ProbeProtocol.self)
        connection.exportedObject = service
        lock.lock(); connections.append(connection); lock.unlock()
        connection.activate()
        return true
    }

    func invalidateConnections() {
        lock.lock(); let active = connections; lock.unlock()
        for connection in active { connection.invalidate() }
    }
}

final class PeerAuthenticationTests: XCTestCase {
    func testFixedRequirementsCompileAndRejectTheTestHost() throws {
        _ = try ExternalUpdaterPeerAuthentication()
        var code: SecCode?
        XCTAssertEqual(SecCodeCopySelf([], &code), errSecSuccess)
        let current = try XCTUnwrap(code)
        for text in [ExternalUpdaterPeerAuthentication.applicationRequirement,
                     ExternalUpdaterPeerAuthentication.helperRequirement] {
            var requirement: SecRequirement?
            XCTAssertEqual(SecRequirementCreateWithString(text as CFString, [], &requirement), errSecSuccess)
            XCTAssertNotEqual(SecCodeCheckValidity(current, [], try XCTUnwrap(requirement)), errSecSuccess)
        }
    }

    func testIdentityContractDoesNotAcceptSiblingAppsOrDebugBuilds() {
        let app = ExternalUpdaterPeerAuthentication.applicationRequirement
        let helper = ExternalUpdaterPeerAuthentication.helperRequirement
        XCTAssertTrue(app.contains("identifier \"com.jasoncavinder.Helm\""))
        XCTAssertTrue(helper.contains("identifier \"com.jasoncavinder.Helm.SparkleExternalUpdater\""))
        XCTAssertTrue(app.contains("info[HelmDistributionChannel] = \"developer_id\""))
        XCTAssertTrue(app.contains("and entitlement[\"com.apple.security.app-sandbox\"] exists"))
        XCTAssertTrue(helper.contains("and ! entitlement[\"com.apple.security.app-sandbox\"] exists"))
        for text in [app, helper] {
            XCTAssertTrue(text.contains("certificate leaf[subject.OU] = \"V73WPJR9M4\""))
            XCTAssertTrue(text.contains("and notarized"))
            XCTAssertTrue(text.contains("and ! entitlement[\"com.apple.security.get-task-allow\"] exists"))
            XCTAssertFalse(text.contains("*"))
            XCTAssertFalse(text.contains(" or "))
        }
    }

    func testAnonymousTransportPositiveControl() throws {
        let delegate = ProbeDelegate()
        let listener = NSXPCListener.anonymous()
        listener.delegate = delegate
        listener.activate()
        let connection = NSXPCConnection(listenerEndpoint: listener.endpoint)
        connection.remoteObjectInterface = NSXPCInterface(with: ProbeProtocol.self)
        connection.activate()
        defer { connection.invalidate(); delegate.invalidateConnections(); listener.invalidate() }
        let reply = expectation(description: "unrestricted test transport works")
        let proxy = try XCTUnwrap(connection.remoteObjectProxyWithErrorHandler { error in
            XCTFail("control transport failed: \(error)")
            reply.fulfill()
        } as? ProbeProtocol)
        proxy.ping { value in XCTAssertEqual(value, "pong"); reply.fulfill() }
        wait(for: [reply], timeout: 5)
        XCTAssertEqual(delegate.service.count, 1)
    }

    func testListenerRejectsUntrustedPeerBeforeDelegateOrRequestDelivery() throws {
        let authentication = try ExternalUpdaterPeerAuthentication()
        let delegate = ProbeDelegate(authentication: authentication)
        let listener = authentication.makeListener(delegate: delegate)
        listener.activate()
        let connection = NSXPCConnection(listenerEndpoint: listener.endpoint)
        connection.remoteObjectInterface = NSXPCInterface(with: ProbeProtocol.self)
        let rejected = expectation(description: "OS rejects non-Helm peer")
        connection.activate()
        defer { connection.invalidate(); delegate.invalidateConnections(); listener.invalidate() }
        let proxy = try XCTUnwrap(connection.remoteObjectProxyWithErrorHandler { error in
            XCTAssertEqual((error as NSError).domain, NSCocoaErrorDomain)
            rejected.fulfill()
        } as? ProbeProtocol)
        proxy.ping { _ in XCTFail("untrusted peer received a reply") }
        wait(for: [rejected], timeout: 5)
        XCTAssertEqual(delegate.count, 0)
        XCTAssertEqual(delegate.service.count, 0)
    }

    func testClientRejectsImpostorHelperResponse() throws {
        let authentication = try ExternalUpdaterPeerAuthentication()
        let delegate = ProbeDelegate()
        let listener = NSXPCListener.anonymous()
        listener.delegate = delegate
        listener.activate()
        let connection = authentication.makeConnection(to: listener.endpoint)
        connection.remoteObjectInterface = NSXPCInterface(with: ProbeProtocol.self)
        let rejected = expectation(description: "OS rejects non-helper peer")
        connection.activate()
        defer { connection.invalidate(); delegate.invalidateConnections(); listener.invalidate() }
        let proxy = try XCTUnwrap(connection.remoteObjectProxyWithErrorHandler { error in
            XCTAssertEqual((error as NSError).domain, NSCocoaErrorDomain)
            rejected.fulfill()
        } as? ProbeProtocol)
        proxy.ping { _ in XCTFail("impostor helper response reached the client") }
        wait(for: [rejected], timeout: 5)
    }

    func testAcceptedConnectionStillChecksEveryIncomingPeer() throws {
        let authentication = try ExternalUpdaterPeerAuthentication()
        let delegate = ProbeDelegate(authentication: authentication)
        // Deliberately omit the listener-level gate in this test so that the
        // incoming connection's independent requirement is actually exercised.
        let listener = NSXPCListener.anonymous()
        listener.delegate = delegate
        listener.activate()
        let connection = NSXPCConnection(listenerEndpoint: listener.endpoint)
        connection.remoteObjectInterface = NSXPCInterface(with: ProbeProtocol.self)
        connection.activate()
        defer { connection.invalidate(); delegate.invalidateConnections(); listener.invalidate() }
        let rejected = expectation(description: "per-connection identity rejects test host")
        let proxy = try XCTUnwrap(connection.remoteObjectProxyWithErrorHandler { _ in rejected.fulfill() } as? ProbeProtocol)
        proxy.ping { _ in XCTFail("untrusted request reached exported object") }
        wait(for: [rejected], timeout: 5)
        XCTAssertEqual(delegate.count, 1)
        XCTAssertEqual(delegate.service.count, 0)
    }
}
