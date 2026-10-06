import Foundation
import HelmExternalUpdateObservation

/// One accepted connection per process, at most 120 seconds. App preflight is
/// read-only; no persistent registration, retry, grant or installer is exposed.
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

    func run(bundledService: Bool = false) {
        let listener = bundledService
            ? authentication.makeBundledServiceListener(delegate: self)
            : authentication.makeBootstrapServiceListener(delegate: self)
        self.listener = listener
        DispatchQueue.main.asyncAfter(deadline: .now() + .seconds(120)) { [weak self] in
            self?.listener?.invalidate()
            exit(1)
        }
        // service() may own the main run loop. Arm the deadline before resuming.
        listener.resume()
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
            let server = try ExternalUpdaterBootstrapServer(connection: connection, helperIdentity: current) { event in
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
