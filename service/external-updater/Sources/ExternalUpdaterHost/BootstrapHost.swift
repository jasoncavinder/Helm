import Foundation
import HelmExternalUpdateObservation

/// One accepted connection per process, at most 120 seconds. No operational
/// requests, persistent registration, retry or installer are introduced here.
final class BootstrapHost: NSObject, NSXPCListenerDelegate {
    private let identity: NativeHelperEvidence
    private let authentication: ExternalUpdaterPeerAuthentication
    private var listener: NSXPCListener?
    private let lock = NSLock()
    private var accepted = false
    private var server: ExternalUpdaterBootstrapServer?

    init(identity: NativeHelperEvidence) throws {
        self.identity = identity
        authentication = try ExternalUpdaterPeerAuthentication()
    }

    func run() {
        let listener = authentication.makeBootstrapServiceListener(delegate: self)
        self.listener = listener
        listener.activate()
        DispatchQueue.main.asyncAfter(deadline: .now() + .seconds(120)) { [weak self] in
            self?.listener?.invalidate()
            exit(1)
        }
        withExtendedLifetime(self) { RunLoop.main.run() }
    }

    func listener(_ listener: NSXPCListener, shouldAcceptNewConnection connection: NSXPCConnection) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        guard !accepted else { connection.invalidate(); return false }
        do {
            let current = try NativeHelperObserver().observeSelf()
            guard current.canonicalPath == identity.canonicalPath,
                  current.codeDirectoryHash == identity.codeDirectoryHash,
                  current.build == identity.build, current.account == identity.account else {
                connection.invalidate()
                return false
            }
            let server = try ExternalUpdaterBootstrapServer(connection: connection) { event in
                switch event {
                case .ready:
                    FileHandle.standardOutput.write(Data("{\"event\":\"authenticated_caller_ready\"}\n".utf8))
                case .closed:
                    DispatchQueue.main.async { listener.invalidate(); exit(0) }
                }
            }
            self.server = server
            accepted = true
            return true
        } catch {
            connection.invalidate()
            return false
        }
    }
}
