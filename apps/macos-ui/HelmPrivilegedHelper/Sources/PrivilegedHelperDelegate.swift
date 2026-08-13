import Foundation
import os.log

private let delegateLogger = Logger(
    subsystem: PrivilegedHelperConstants.helperIdentifier,
    category: "delegate"
)

final class PrivilegedHelperDelegate: NSObject, NSXPCListenerDelegate {
    func listener(_ listener: NSXPCListener, shouldAcceptNewConnection connection: NSXPCConnection) -> Bool {
        let clientPID = connection.processIdentifier
        guard CodeSigningValidator.process(
            clientPID,
            satisfiesIdentifier: PrivilegedHelperConstants.helperIdentifier
        ),
        let parentPID = CodeSigningValidator.parentProcessIdentifier(of: clientPID),
        CodeSigningValidator.process(
            parentPID,
            satisfiesIdentifier: PrivilegedHelperConstants.serviceIdentifier
        ) else {
            delegateLogger.warning("Rejected privileged helper client PID \(clientPID)")
            return false
        }

        let service = PrivilegedHelperService()
        connection.exportedInterface = NSXPCInterface(with: HelmPrivilegedHelperProtocol.self)
        connection.exportedObject = service
        connection.interruptionHandler = { service.cancel() }
        connection.invalidationHandler = { service.cancel() }
        connection.resume()
        return true
    }
}
