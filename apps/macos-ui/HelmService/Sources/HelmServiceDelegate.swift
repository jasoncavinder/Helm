import Foundation
import Security
import os.log

private let logger = Logger(subsystem: "app.jasoncavinder.Helm.HelmService", category: "delegate")

/// The signing identity required for connections to the embedded Helm service.
private let expectedTeamID = "V73WPJR9M4"
private let expectedHelmBundleIdentifier = "com.jasoncavinder.Helm"

class HelmServiceDelegate: NSObject, NSXPCListenerDelegate {
    private let sharedService = HelmService()

    func listener(_ listener: NSXPCListener, shouldAcceptNewConnection newConnection: NSXPCConnection) -> Bool {
        guard validateConnection(newConnection) else {
            logger.warning("Rejected XPC connection from PID \(newConnection.processIdentifier)")
            return false
        }

        newConnection.exportedInterface = NSXPCInterface(with: HelmServiceProtocol.self)
        newConnection.exportedObject = sharedService
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

        // Only the Helm app, not another app signed by the same team, may use this service.
        var requirement: SecRequirement?
        let requirementString = "anchor apple generic and certificate leaf[subject.OU] = \"\(expectedTeamID)\" and identifier \"\(expectedHelmBundleIdentifier)\"" as CFString
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
