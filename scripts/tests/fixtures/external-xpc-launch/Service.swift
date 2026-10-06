import Darwin
import Foundation

final class ProbeService: NSObject, NSXPCListenerDelegate, LaunchProbeProtocol {
    private var accepted = false
    func listener(_ listener: NSXPCListener, shouldAcceptNewConnection connection: NSXPCConnection) -> Bool {
        guard !accepted, connection.effectiveUserIdentifier == geteuid() else { return false }
        accepted = true
        connection.setCodeSigningRequirement("identifier \"com.jasoncavinder.Helm.XPCLaunchProbe\"")
        connection.exportedInterface = NSXPCInterface(with: LaunchProbeProtocol.self)
        connection.exportedObject = self
        connection.invalidationHandler = { exit(0) }
        connection.activate()
        return true
    }
    func inspect(reply: @escaping (Data) -> Void) {
        reply((try? JSONEncoder().encode(readSentinel(host: false))) ?? Data())
    }
}

@main struct ServiceMain {
    static func main() throws {
        if CommandLine.arguments == [CommandLine.arguments[0], "--child"] {
            FileHandle.standardOutput.write(try JSONEncoder().encode(readSentinel(host: false)))
            return
        }
        guard CommandLine.arguments.count == 1 else { exit(64) }
        let delegate = ProbeService()
        let listener = NSXPCListener.service()
        // Service listeners do not support listener-level requirements. The
        // delegate sets the connection requirement before activation instead.
        listener.delegate = delegate
        DispatchQueue.global().asyncAfter(deadline: .now() + .seconds(30)) { exit(1) }
        withExtendedLifetime(delegate) { listener.resume() }
    }
}
