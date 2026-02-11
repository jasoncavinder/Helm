import Foundation

class HelmServiceDelegate: NSObject, NSXPCListenerDelegate {
    func listener(_ listener: NSXPCListener, shouldAcceptNewConnection newConnection: NSXPCConnection) -> Bool {
        newConnection.exportedInterface = NSXPCInterface(with: HelmServiceProtocol.self)
        newConnection.exportedObject = HelmService()
        newConnection.resume()
        return true
    }
}
