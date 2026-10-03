import Darwin
import Foundation
import Security

public enum PeerAuthenticationFailure: String, Error {
    case invalidBuiltInRequirement
    case invalidAccount
}

/// Fixed production identities, never request parameters, preferences or a PID
/// lookup. The OS checks the live peer when delivering connection messages.
public struct ExternalUpdaterPeerAuthentication {
    public static let bootstrapServiceName = "com.jasoncavinder.Helm.SparkleExternalUpdater.bootstrap"
    static let applicationRequirement = """
        anchor apple generic
        and certificate 1[field.1.2.840.113635.100.6.2.6] exists
        and certificate leaf[field.1.2.840.113635.100.6.1.13] exists
        and certificate leaf[subject.OU] = "V73WPJR9M4"
        and identifier "com.jasoncavinder.Helm"
        and info[HelmDistributionChannel] = "developer_id"
        and entitlement["com.apple.security.app-sandbox"] exists
        and ! entitlement["com.apple.security.get-task-allow"] exists
        and ! entitlement["com.apple.security.cs.disable-library-validation"] exists
        and ! entitlement["com.apple.security.cs.allow-dyld-environment-variables"] exists
        and notarized
        """
    static let helperRequirement = """
        anchor apple generic
        and certificate 1[field.1.2.840.113635.100.6.2.6] exists
        and certificate leaf[field.1.2.840.113635.100.6.1.13] exists
        and certificate leaf[subject.OU] = "V73WPJR9M4"
        and identifier "com.jasoncavinder.Helm.SparkleExternalUpdater"
        and ! entitlement["com.apple.security.app-sandbox"] exists
        and ! entitlement["com.apple.security.get-task-allow"] exists
        and ! entitlement["com.apple.security.cs.disable-library-validation"] exists
        and ! entitlement["com.apple.security.cs.allow-dyld-environment-variables"] exists
        and notarized
        """

    private let account: uid_t

    public init() throws {
        guard geteuid() != 0, getuid() == geteuid() else { throw PeerAuthenticationFailure.invalidAccount }
        account = geteuid()
        // Validate before Foundation's setters, which throw Objective-C
        // exceptions for malformed requirements. There is no permissive fallback.
        for text in [Self.applicationRequirement, Self.helperRequirement] {
            var requirement: SecRequirement?
            guard SecRequirementCreateWithString(text as CFString, [], &requirement) == errSecSuccess,
                  requirement != nil else { throw PeerAuthenticationFailure.invalidBuiltInRequirement }
        }
    }

    /// Returns inactive. The fixed listener requirement rejects an untrusted
    /// peer before the delegate runs. The delegate must still call admit before
    /// setting an exported object or activating each accepted connection.
    public func makeListener(delegate: NSXPCListenerDelegate) -> NSXPCListener {
        let listener = NSXPCListener.anonymous()
        listener.setConnectionCodeSigningRequirement(Self.applicationRequirement)
        listener.delegate = delegate
        return listener
    }

    /// Registration/launch is a separate packaging concern. This fixed-name
    /// listener retains the same gate as the anonymous bootstrap transport.
    public func makeBootstrapServiceListener(delegate: NSXPCListenerDelegate) -> NSXPCListener {
        let listener = NSXPCListener(machServiceName: Self.bootstrapServiceName)
        listener.setConnectionCodeSigningRequirement(Self.applicationRequirement)
        listener.delegate = delegate
        return listener
    }

    /// Call once, on a newly received inactive connection. A true return only
    /// means the OS identity gate was configured, not that a message has passed
    /// authentication or that any operation is authorized.
    public func admit(_ connection: NSXPCConnection) -> Bool {
        guard connection.effectiveUserIdentifier == account else {
            connection.invalidate()
            return false
        }
        connection.setCodeSigningRequirement(Self.applicationRequirement)
        Self.closeOnInterruption(connection)
        return true
    }

    /// Returns inactive and without interfaces. Only the exact separately
    /// signed/notarized helper may respond after the caller configures it.
    public func makeConnection(to endpoint: NSXPCListenerEndpoint) -> NSXPCConnection {
        let connection = NSXPCConnection(listenerEndpoint: endpoint)
        connection.setCodeSigningRequirement(Self.helperRequirement)
        Self.closeOnInterruption(connection)
        return connection
    }

    public func makeBootstrapServiceConnection() -> NSXPCConnection {
        let connection = NSXPCConnection(machServiceName: Self.bootstrapServiceName, options: [])
        connection.setCodeSigningRequirement(Self.helperRequirement)
        Self.closeOnInterruption(connection)
        return connection
    }

    private static func closeOnInterruption(_ connection: NSXPCConnection) {
        // NSXPC otherwise permits reconnection after interruption. A future
        // operation must get a new review, not silently reuse the old session.
        connection.interruptionHandler = { [weak connection] in connection?.invalidate() }
    }
}
