import Foundation

/// Created only by the delegate of the owned service() listener. An arbitrary
/// helper path, a Mach listener or an inactive connection is not launch proof.
struct PrivateServiceAcceptance {
    let connection: NSXPCConnection
    let identity: NativeHelperEvidence

    fileprivate init(connection: NSXPCConnection, identity: NativeHelperEvidence) {
        self.connection = connection
        self.identity = identity
    }
}

/// Staged, unembedded private service. One connection and no installer/adoption
/// activation. The listener and its acceptance context cannot be supplied by IPC.
public final class ExternalUpdaterPrivateServiceHost: NSObject, NSXPCListenerDelegate {
    private let identity: NativeHelperEvidence
    private let authentication: ExternalUpdaterPeerAuthentication
    private let event: (BootstrapEvent) -> Void
    private var listener: NSXPCListener?
    private var server: ExternalUpdaterBootstrapServer?
    private let lock = NSLock()
    private var accepted = false

    private init(identity: NativeHelperEvidence, event: @escaping (BootstrapEvent) -> Void) throws {
        try HelperBundleFormat.privateService.validate(path: URL(fileURLWithPath: identity.canonicalPath))
        self.identity = identity
        self.event = event
        authentication = try ExternalUpdaterPeerAuthentication()
    }

    public static func run(identity: NativeHelperEvidence, event: @escaping (BootstrapEvent) -> Void) throws {
        let host = try ExternalUpdaterPrivateServiceHost(identity: identity, event: event)
        let listener = host.authentication.makeBundledServiceListener(delegate: host)
        host.listener = listener
        DispatchQueue.main.asyncAfter(deadline: .now() + .seconds(120)) { [weak host] in
            host?.listener?.invalidate()
            exit(1)
        }
        withExtendedLifetime(host) { listener.resume(); RunLoop.main.run() }
    }

    public func listener(_ listener: NSXPCListener, shouldAcceptNewConnection connection: NSXPCConnection) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        guard listener === self.listener, !accepted else { connection.invalidate(); return false }
        do {
            let current = try NativeHelperObserver().observeSelf()
            guard current == identity else { throw HelperObservationFailure.changedDuringObservation }
            let acceptance = PrivateServiceAcceptance(connection: connection, identity: current)
            server = try ExternalUpdaterBootstrapServer(privateService: acceptance) { [event] value in
                event(value)
                if case .closed = value {
                    DispatchQueue.main.async { listener.invalidate(); exit(0) }
                }
            }
            accepted = true
            return true
        } catch {
            connection.invalidate()
            return false
        }
    }
}
