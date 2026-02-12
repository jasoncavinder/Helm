import Foundation
import Security
import os.log

private let logger = Logger(subsystem: "app.jasoncavinder.Helm.HelmService", category: "delegate")

/// The development team ID used for code signing validation.
/// Only processes signed by this team are allowed to connect to the XPC service.
private let expectedTeamID = "V73WPJR9M4"

class HelmServiceDelegate: NSObject, NSXPCListenerDelegate {
    func listener(_ listener: NSXPCListener, shouldAcceptNewConnection newConnection: NSXPCConnection) -> Bool {
        guard validateConnection(newConnection) else {
            logger.warning("Rejected XPC connection from PID \(newConnection.processIdentifier)")
            return false
        }

        newConnection.exportedInterface = NSXPCInterface(with: HelmServiceProtocol.self)
        newConnection.exportedObject = HelmService()
        newConnection.resume()
        return true
    }

    private func validateConnection(_ connection: NSXPCConnection) -> Bool {
        let pid = connection.processIdentifier

        var code: SecCode?
        let attributes = [kSecGuestAttributePid: pid] as NSDictionary as CFDictionary
        guard SecCodeCopyGuestWithAttributes(nil, attributes, SecCSFlags(), &code) == errSecSuccess,
              let code = code else {
            logger.warning("Failed to create SecCode for PID \(pid)")
            return false
        }

        // Require the connecting process is signed by the same development team
        var requirement: SecRequirement?
        let requirementString = "anchor apple generic and certificate leaf[subject.OU] = \"\(expectedTeamID)\"" as CFString
        guard SecRequirementCreateWithString(requirementString, SecCSFlags(), &requirement) == errSecSuccess,
              let requirement = requirement else {
            logger.warning("Failed to create security requirement")
            return false
        }

        let result = SecCodeCheckValidity(code, SecCSFlags(), requirement)
        if result != errSecSuccess {
            logger.warning("Connection from PID \(pid) failed code signing validation (OSStatus: \(result))")
            return false
        }

        return true
    }
}
