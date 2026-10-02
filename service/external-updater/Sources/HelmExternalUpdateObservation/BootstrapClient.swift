import Darwin
import Foundation

public final class ExternalUpdaterBootstrapClient {
    private let connection: NSXPCConnection
    private let queue = DispatchQueue(label: "com.jasoncavinder.Helm.external-bootstrap-client")
    private let notifications = DispatchQueue(label: "com.jasoncavinder.Helm.external-bootstrap-client-events")
    private let event: (BootstrapEvent) -> Void
    private var gate: BootstrapReplyGate
    private var started = false

    /// Retain until finished; release/cancel closes the connection. The endpoint
    /// alone is untrusted. Production always installs the fixed helper requirement.
    public convenience init(endpoint: NSXPCListenerEndpoint, event: @escaping (BootstrapEvent) -> Void) throws {
        let authentication = try ExternalUpdaterPeerAuthentication()
        try self.init(connection: authentication.makeConnection(to: endpoint), event: event)
    }

    /// The named service is still untrusted until the data-free handshake and
    /// native peer/account validation complete. No operational API is exposed.
    public convenience init(event: @escaping (BootstrapEvent) -> Void) throws {
        let authentication = try ExternalUpdaterPeerAuthentication()
        try self.init(connection: authentication.makeBootstrapServiceConnection(), event: event)
    }

    private init(connection: NSXPCConnection, event: @escaping (BootstrapEvent) -> Void) throws {
        self.connection = connection
        self.event = event
        gate = BootstrapReplyGate(challenge: try BootstrapWire.nonce(), account: geteuid(),
                                  started: DispatchTime.now().uptimeNanoseconds)
        connection.remoteObjectInterface = NSXPCInterface(with: ExternalUpdaterBootstrapProtocol.self)
        connection.interruptionHandler = { [weak self] in self?.close(.interrupted) }
        connection.invalidationHandler = { [weak self] in self?.close(.invalidated) }
    }

    #if DEBUG
    // Test-only unrestricted transport isolates protocol behavior from identity
    // rejection. Release builds have no alternate connection constructor.
    convenience init(testingConnection: NSXPCConnection, event: @escaping (BootstrapEvent) -> Void) throws {
        try self.init(connection: testingConnection, event: event)
    }
    #endif

    deinit { connection.invalidate() }

    public func begin() {
        queue.async { [weak self] in
            guard let self, !self.started, !self.gate.closed else { return }
            self.started = true
            self.gate = BootstrapReplyGate(challenge: self.gate.challenge, account: geteuid(),
                                           started: DispatchTime.now().uptimeNanoseconds)
            self.connection.activate()
            self.queue.asyncAfter(deadline: .now() + .seconds(5)) { [weak self] in
                guard let self, !self.gate.established else { return }
                self.finish(.expired)
            }
            self.queue.asyncAfter(deadline: .now() + .seconds(120)) { [weak self] in self?.finish(.expired) }
            guard let proxy = self.connection.remoteObjectProxyWithErrorHandler({ [weak self] _ in
                self?.close(.transport)
            }) as? ExternalUpdaterBootstrapProtocol else {
                self.finish(.transport)
                return
            }
            proxy.hello(version: BootstrapWire.version, challenge: self.gate.challenge) { [weak self] version, echo, session in
                self?.receive(version: version, echo: echo, session: session)
            }
        }
    }

    public func cancel() { close(.cancelled) }

    private func receive(version: UInt32, echo: Data?, session: Data?) {
        queue.async { [weak self] in
            guard let self, !self.gate.closed else { return }
            do {
                try self.gate.accept(version: version, echo: echo, session: session,
                                     peerAccount: self.connection.effectiveUserIdentifier,
                                     now: DispatchTime.now().uptimeNanoseconds)
                self.emit(.ready)
            } catch let error as BootstrapFailure {
                self.finish(error)
            } catch {
                self.finish(.invalidMessage)
            }
        }
    }

    private func close(_ failure: BootstrapFailure) {
        queue.async { [weak self] in self?.finish(failure) }
    }

    private func finish(_ failure: BootstrapFailure) {
        guard gate.close() else { return }
        connection.invalidate()
        emit(.closed(failure))
    }

    private func emit(_ value: BootstrapEvent) {
        // Consumer callbacks must not delay timeout or connection invalidation.
        notifications.async { [event] in event(value) }
    }
}
