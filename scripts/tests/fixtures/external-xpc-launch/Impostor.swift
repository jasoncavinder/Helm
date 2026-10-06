import Darwin
import Foundation

// VM-only, intentionally wrong signing identity. Never linked into the helper.
@objc(HELMExternalUpdaterBootstrapProtocol)
protocol ImpostorProtocol {
    func hello(version: UInt32, challenge: Data, reply: @escaping (UInt32, Data?, Data?) -> Void)
    func preflight(session: Data, sequence: UInt64, request: Data, reply: @escaping (UInt64, UInt32) -> Void)
}

private func record(_ name: String) {
    var root = Bundle.main.bundleURL
    for _ in 0..<4 { root.deleteLastPathComponent() }
    let path = root.appendingPathComponent(name, isDirectory: false).path
    let descriptor = open(path, O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC | O_NOFOLLOW, 0o600)
    guard descriptor >= 0 else { exit(1) }
    close(descriptor)
}

final class Impostor: NSObject, ImpostorProtocol, NSXPCListenerDelegate {
    private var accepted = false
    func listener(_ listener: NSXPCListener, shouldAcceptNewConnection connection: NSXPCConnection) -> Bool {
        guard !accepted else { return false }
        accepted = true
        connection.exportedInterface = NSXPCInterface(with: ImpostorProtocol.self)
        connection.exportedObject = self
        connection.invalidationHandler = { exit(0) }
        connection.activate()
        return true
    }

    func hello(version: UInt32, challenge: Data, reply: @escaping (UInt32, Data?, Data?) -> Void) {
        record("impostor-hello")
        reply(version, challenge, Data(repeating: 7, count: 32))
    }

    func preflight(session: Data, sequence: UInt64, request: Data, reply: @escaping (UInt64, UInt32) -> Void) {
        record("impostor-target-received")
        reply(sequence, 1)
    }
}

@main struct ImpostorMain {
    static func main() {
        record("impostor-started")
        let delegate = Impostor()
        let listener = NSXPCListener.service()
        listener.delegate = delegate
        DispatchQueue.global().asyncAfter(deadline: .now() + .seconds(20)) { exit(1) }
        withExtendedLifetime(delegate) { listener.resume() }
    }
}
