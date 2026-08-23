import Combine
import Foundation
import os.log
import ServiceManagement

private let privilegedHelperRegistrationLogger = Logger(
    subsystem: "com.jasoncavinder.Helm",
    category: "privileged-helper-registration"
)

enum PrivilegedHelperRegistrationAvailability: Equatable {
    case available
    case unsupportedChannel
    case missingArtifacts
}

enum PrivilegedHelperRegistrationStatus: Equatable {
    case unavailable
    case notRegistered
    case enabled
    case requiresApproval
    case notFound
}

enum PrivilegedHelperOperationError: Equatable {
    case registrationFailed
    case unregistrationFailed
}

enum PrivilegedHelperRegistrationPolicy {
    static func availability(
        channel: HelmDistributionChannel,
        helperExecutableExists: Bool,
        launchDaemonPlistExists: Bool
    ) -> PrivilegedHelperRegistrationAvailability {
        guard channel == .developerID else {
            return .unsupportedChannel
        }
        guard helperExecutableExists, launchDaemonPlistExists else {
            return .missingArtifacts
        }
        return .available
    }
}

protocol PrivilegedHelperRegistrationServicing: AnyObject {
    var registrationStatus: PrivilegedHelperRegistrationStatus { get }

    func register() throws
    func unregister() throws
    func openSystemSettingsLoginItems()
}

final class SMPrivilegedHelperService: PrivilegedHelperRegistrationServicing {
    private let service: SMAppService

    init(plistName: String) {
        service = SMAppService.daemon(plistName: plistName)
    }

    var registrationStatus: PrivilegedHelperRegistrationStatus {
        switch service.status {
        case .notRegistered:
            return .notRegistered
        case .enabled:
            return .enabled
        case .requiresApproval:
            return .requiresApproval
        case .notFound:
            return .notFound
        @unknown default:
            return .notFound
        }
    }

    func register() throws {
        try service.register()
    }

    func unregister() throws {
        try service.unregister()
    }

    func openSystemSettingsLoginItems() {
        SMAppService.openSystemSettingsLoginItems()
    }
}

final class PrivilegedHelperRegistrationController: ObservableObject {
    static let launchDaemonPlistName = "com.jasoncavinder.Helm.PrivilegedHelper.plist"
    static let helperExecutableRelativePath = "Contents/Library/LaunchServices/HelmPrivilegedHelper"
    static let launchDaemonPlistRelativePath =
        "Contents/Library/LaunchDaemons/\(launchDaemonPlistName)"

    static let shared = production()

    @Published private(set) var status: PrivilegedHelperRegistrationStatus
    @Published private(set) var operationInProgress = false
    @Published private(set) var operationError: PrivilegedHelperOperationError?

    let availability: PrivilegedHelperRegistrationAvailability

    private let service: PrivilegedHelperRegistrationServicing?
    private let blocksLiveOperations: () -> Bool

    var shouldPresentSettings: Bool {
        availability != .unsupportedChannel && !blocksLiveOperations()
    }

    init(
        availability: PrivilegedHelperRegistrationAvailability,
        service: PrivilegedHelperRegistrationServicing?,
        blocksLiveOperations: @escaping () -> Bool = { false }
    ) {
        self.availability = availability
        self.service = service
        self.blocksLiveOperations = blocksLiveOperations
        if blocksLiveOperations() {
            status = .unavailable
        } else if availability == .available {
            status = service?.registrationStatus ?? .notFound
        } else {
            status = .unavailable
        }
    }

    static func production(
        bundle: Bundle = .main,
        fileManager: FileManager = .default,
        blocksLiveOperations: @escaping () -> Bool = {
            ResearchFixtureSafetyPolicy.blocksLiveOperations()
        }
    ) -> PrivilegedHelperRegistrationController {
        let helperURL = bundle.bundleURL.appendingPathComponent(helperExecutableRelativePath)
        let plistURL = bundle.bundleURL.appendingPathComponent(launchDaemonPlistRelativePath)
        let availability = PrivilegedHelperRegistrationPolicy.availability(
            channel: HelmDistributionChannel.from(bundle: bundle),
            helperExecutableExists: fileManager.isExecutableFile(atPath: helperURL.path),
            launchDaemonPlistExists: fileManager.fileExists(atPath: plistURL.path)
        )
        let service: PrivilegedHelperRegistrationServicing? = availability == .available
            && !blocksLiveOperations()
            ? SMPrivilegedHelperService(plistName: launchDaemonPlistName)
            : nil
        return PrivilegedHelperRegistrationController(
            availability: availability,
            service: service,
            blocksLiveOperations: blocksLiveOperations
        )
    }

    func refresh() {
        operationError = nil
        guard !blocksLiveOperations() else {
            status = .unavailable
            return
        }
        refreshStatus()
    }

    func register() {
        guard !blocksLiveOperations(),
              availability == .available,
              let service,
              !operationInProgress else {
            return
        }

        operationInProgress = true
        operationError = nil
        var shouldOpenLoginItems = false
        do {
            try service.register()
        } catch {
            refreshStatus()
            if status == .requiresApproval {
                shouldOpenLoginItems = true
            } else if status != .enabled, isApprovalRequiredError(error) {
                status = .requiresApproval
                shouldOpenLoginItems = true
            } else if status != .enabled {
                operationError = .registrationFailed
                privilegedHelperRegistrationLogger.error(
                    "Privileged helper registration failed: \(error.localizedDescription, privacy: .public)"
                )
            }
        }

        if operationError == nil, !shouldOpenLoginItems {
            refreshStatus()
            shouldOpenLoginItems = status == .requiresApproval
        }

        operationInProgress = false

        if shouldOpenLoginItems {
            service.openSystemSettingsLoginItems()
        }
    }

    func unregister() {
        guard !blocksLiveOperations(),
              availability == .available,
              let service,
              !operationInProgress else {
            return
        }

        operationInProgress = true
        operationError = nil
        do {
            try service.unregister()
        } catch {
            refreshStatus()
            if status != .notRegistered {
                operationError = .unregistrationFailed
                privilegedHelperRegistrationLogger.error(
                    "Privileged helper unregistration failed: \(error.localizedDescription, privacy: .public)"
                )
            }
        }
        refreshStatus()
        operationInProgress = false
    }

    func openSystemSettingsLoginItems() {
        guard !blocksLiveOperations(), availability == .available else { return }
        service?.openSystemSettingsLoginItems()
    }

    private func refreshStatus() {
        guard !blocksLiveOperations(), availability == .available else {
            status = .unavailable
            return
        }
        status = service?.registrationStatus ?? .notFound
    }

    private func isApprovalRequiredError(_ error: Error) -> Bool {
        let nsError = error as NSError
        return nsError.code == Int(kSMErrorLaunchDeniedByUser)
    }
}
