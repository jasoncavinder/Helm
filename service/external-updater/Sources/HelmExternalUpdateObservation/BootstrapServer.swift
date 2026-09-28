import Darwin
import Foundation

/// Per-connection, data-free bootstrap only. The future signed helper must use
/// the gated listener and retain this object for each newly accepted connection.
/// No listener, helper process, filesystem observation or installer is launched.
public final class ExternalUpdaterBootstrapServer: NSObject, ExternalUpdaterBootstrapProtocol {
    private let connection: NSXPCConnection
    private let queue = DispatchQueue(label: "com.jasoncavinder.Helm.external-bootstrap-server")
    private let notifications = DispatchQueue(label: "com.jasoncavinder.Helm.external-bootstrap-server-events")
    private let event: (BootstrapEvent) -> Void
    private let started = DispatchTime.now().uptimeNanoseconds
    private var established = false
    private var closed = false

    /// `connection` must be newly accepted and inactive. Do not call admit first:
    /// Foundation permits configuring a connection requirement only once.
    public convenience init(connection: NSXPCConnection, event: @escaping (BootstrapEvent) -> Void) throws {
        let authentication = try ExternalUpdaterPeerAuthentication()
        guard authentication.admit(connection) else { throw BootstrapFailure.invalidAccount }
        self.init(configuredConnection: connection, event: event)
    }

    private init(configuredConnection: NSXPCConnection, event: @escaping (BootstrapEvent) -> Void) {
        connection = configuredConnection
        self.event = event
        super.init()
        connection.exportedInterface = NSXPCInterface(with: ExternalUpdaterBootstrapProtocol.self)
        connection.exportedObject = self
        connection.interruptionHandler = { [weak self] in self?.close(.interrupted) }
        connection.invalidationHandler = { [weak self] in self?.close(.invalidated) }
        queue.asyncAfter(deadline: .now() + .seconds(5)) { [weak self] in
            guard let self, !self.established else { return }
            self.finish(.expired)
        }
        queue.asyncAfter(deadline: .now() + .seconds(120)) { [weak self] in self?.finish(.expired) }
        connection.activate()
    }

    #if DEBUG
    convenience init(testingConnection: NSXPCConnection, event: @escaping (BootstrapEvent) -> Void) {
        self.init(configuredConnection: testingConnection, event: event)
    }
    #endif

    public func hello(version: UInt32, challenge: Data, reply: @escaping (UInt32, Data?, Data?) -> Void) {
        queue.async { [weak self] in
            guard let self else { return }
            let now = DispatchTime.now().uptimeNanoseconds
            guard !self.closed, !self.established, version == BootstrapWire.version,
                  challenge.count == BootstrapWire.bytes else {
                reply(0, nil, nil)
                self.finish(.invalidMessage)
                return
            }
            guard now >= self.started, now - self.started < BootstrapWire.handshakeNanoseconds else {
                reply(0, nil, nil)
                self.finish(.expired)
                return
            }
            do {
                let session = try BootstrapWire.nonce()
                self.established = true
                reply(BootstrapWire.version, challenge, session)
                self.emit(.ready)
            } catch {
                reply(0, nil, nil)
                self.finish(.entropyUnavailable)
            }
        }
    }

    public func cancel() { close(.cancelled) }

    private func close(_ failure: BootstrapFailure) {
        queue.async { [weak self] in self?.finish(failure) }
    }

    private func finish(_ failure: BootstrapFailure) {
        guard !closed else { return }
        closed = true
        connection.invalidate()
        connection.exportedObject = nil
        emit(.closed(failure))
    }

    private func emit(_ value: BootstrapEvent) {
        notifications.async { [event] in event(value) }
    }
}
