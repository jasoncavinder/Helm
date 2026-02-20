import AppKit
import Foundation
import os.log

private let logger = Logger(subsystem: "com.jasoncavinder.Helm", category: "core")

struct CorePackageRef: Codable {
    let manager: String
    let name: String
}

struct CoreInstalledPackage: Codable {
    let package: CorePackageRef
    let installedVersion: String?
    let pinned: Bool
}

struct CoreOutdatedPackage: Codable {
    let package: CorePackageRef
    let installedVersion: String?
    let candidateVersion: String
    let pinned: Bool
    let restartRequired: Bool
}

struct CoreTaskRecord: Codable {
    let id: UInt64
    let manager: String
    let taskType: String
    let status: String
    let label: String?
    let labelKey: String?
    let labelArgs: [String: String]?
}

struct CoreSearchResult: Codable {
    let manager: String
    let name: String
    let version: String?
    let summary: String?
    let sourceManager: String
}

struct CoreUpgradePlanStep: Codable, Identifiable {
    let stepId: String
    let orderIndex: UInt64
    let managerId: String
    let authority: String
    let action: String
    let packageName: String
    let reasonLabelKey: String
    let reasonLabelArgs: [String: String]
    let status: String

    var id: String { stepId }
}

struct UpgradePlanTaskProjection {
    let stepId: String
    let taskId: UInt64
    let status: String
    let managerId: String
    let labelKey: String?
}

struct UpgradePlanFailureGroup: Identifiable {
    let id: String
    let managerId: String
    let stepIds: [String]
    let packageNames: [String]
}

enum HomebrewKegPolicyOverride: String, Codable {
    case keep
    case cleanup
}

struct CorePackageKegPolicy: Codable {
    let managerId: String
    let packageName: String
    let policy: HomebrewKegPolicyOverride
}

enum KegPolicySelection {
    case useGlobal
    case keep
    case cleanup
}

struct ManagerStatus: Codable {
    let managerId: String
    let detected: Bool
    let version: String?
    let executablePath: String?
    let enabled: Bool
    let isImplemented: Bool
    let isOptional: Bool
    let isDetectionOnly: Bool
    let supportsRemoteSearch: Bool
    let supportsPackageInstall: Bool
    let supportsPackageUninstall: Bool
    let supportsPackageUpgrade: Bool
}

final class HelmCore: ObservableObject {
    static let shared = HelmCore()

    @Published var isInitialized = false
    @Published var isConnected = false
    @Published var isRefreshing = false
    @Published var isSearching = false
    @Published var searchText: String = "" {
        didSet { onSearchTextChanged(searchText) }
    }
    @Published var installedPackages: [PackageItem] = []
    @Published var outdatedPackages: [PackageItem] = []
    @Published var activeTasks: [TaskItem] = []
    @Published var searchResults: [PackageItem] = []
    @Published var cachedAvailablePackages: [PackageItem] = []
    @Published var upgradePlanSteps: [CoreUpgradePlanStep] = []
    @Published var upgradePlanTaskProjectionByStepId: [String: UpgradePlanTaskProjection] = [:]
    @Published var upgradePlanFailureGroups: [UpgradePlanFailureGroup] = []
    @Published var upgradePlanAllowOsUpdates: Bool = false
    @Published var upgradePlanIncludePinned: Bool = false
    @Published var detectedManagers: Set<String> = []
    @Published var managerStatuses: [String: ManagerStatus] = [:]
    @Published var managerOperations: [String: String] = [:]
    @Published var pinActionPackageIds: Set<String> = []
    @Published var upgradeActionPackageIds: Set<String> = []
    @Published var installActionPackageIds: Set<String> = []
    @Published var uninstallActionPackageIds: Set<String> = []
    @Published var packageDescriptionLoadingIds: Set<String> = []
    @Published var packageDescriptionUnavailableIds: Set<String> = []
    @Published var onboardingDetectionInProgress: Bool = false
    @Published var homebrewKegAutoCleanupEnabled: Bool = false
    @Published var packageKegPolicyOverrides: [String: HomebrewKegPolicyOverride] = [:]
    @Published var safeModeEnabled: Bool = false
    @Published var lastError: String?
    @Published var selectedManagerFilter: String? = nil
    @Published var hasCompletedOnboarding: Bool = UserDefaults.standard.bool(forKey: "hasCompletedOnboarding")

    var timer: Timer?
    var connection: NSXPCConnection?
    var lastRefreshTrigger: Date?
    var searchDebounceTimer: Timer?
    var activeRemoteSearchTaskIds: Set<Int64> = []
    var managerActionTaskDescriptions: [UInt64: String] = [:]
    var managerActionTaskByManager: [String: UInt64] = [:]
    var upgradeActionTaskByPackage: [String: UInt64] = [:]
    var installActionTaskByPackage: [String: UInt64] = [:]
    var uninstallActionTaskByPackage: [String: UInt64] = [:]
    var descriptionLookupTaskIdsByPackage: [String: Set<UInt64>] = [:]
    var descriptionLookupLastAttemptByPackage: [String: Date] = [:]
    var lastObservedTaskId: UInt64 = 0
    var onboardingDetectionAnchorTaskId: UInt64 = 0
    var onboardingDetectionPendingManagers: Set<String> = []
    var onboardingDetectionStartedAt: Date?
    var latestCoreTasksSnapshot: [CoreTaskRecord] = []
    var previousFailedTaskCount: Int = 0
    var previousRefreshState: Bool = false
    private var reconnectAttempt: Int = 0

    private init() {
        setupConnection()
    }

    func setupConnection() {
        let connection = NSXPCConnection(serviceName: "app.jasoncavinder.Helm.HelmService")
        connection.remoteObjectInterface = NSXPCInterface(with: HelmServiceProtocol.self)
        connection.invalidationHandler = { [weak self] in
            logger.error("XPC connection invalidated")
            DispatchQueue.main.async {
                self?.isConnected = false
                self?.clearSearchState()
                self?.scheduleReconnection()
            }
        }
        connection.interruptionHandler = { [weak self] in
            logger.error("XPC connection interrupted")
            DispatchQueue.main.async {
                self?.isConnected = false
                self?.clearSearchState()
                self?.scheduleReconnection()
            }
        }
        connection.resume()
        self.connection = connection

        logger.info("XPC connection established")
        isConnected = true
        isInitialized = true
        reconnectAttempt = 0

        if timer == nil {
            startPolling()
        }
        fetchSafeMode()
        fetchHomebrewKegAutoCleanup()
        fetchPackageKegPolicies()
    }

    func scheduleReconnection() {
        let delay = min(2.0 * pow(2.0, Double(reconnectAttempt)), 60.0)
        reconnectAttempt += 1
        logger.info("Scheduling reconnection in \(delay)s (attempt \(self.reconnectAttempt))")
        DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self] in
            logger.info("Attempting to reconnect...")
            self?.setupConnection()
        }
    }

    func startPolling() {
        timer = Timer.scheduledTimer(withTimeInterval: 1.0, repeats: true) { [weak self] _ in
            self?.fetchTasks()
            self?.fetchPackages()
            self?.fetchOutdatedPackages()
            self?.fetchManagerStatus()
            self?.refreshCachedAvailablePackages()

            // Re-query local cache to pick up enriched results from remote search
            if let query = self?.searchText, !query.trimmingCharacters(in: .whitespaces).isEmpty {
                self?.fetchSearchResults(query: query)
            }
        }
    }

    func service() -> HelmServiceProtocol? {
        return connection?.remoteObjectProxy as? HelmServiceProtocol
    }

    /// Wraps an asynchronous XPC operation with a timeout.
    /// If the operation does not complete within `seconds`, the completion
    /// handler is called with `fallback` and the actual result is discarded.
    func withTimeout<T>(
        _ seconds: TimeInterval,
        operation: @escaping (@escaping (T?) -> Void) -> Void,
        fallback: T? = nil,
        completion: @escaping (T?) -> Void
    ) {
        let completed = DispatchSemaphore(value: 1)
        var hasCompleted = false

        let deadline = DispatchWorkItem { [weak self] in
            completed.wait()
            if !hasCompleted {
                hasCompleted = true
                completed.signal()
                logger.warning("XPC call timed out after \(seconds)s")
                DispatchQueue.main.async {
                    self?.lastError = L10n.Common.error.localized
                    completion(fallback)
                }
            } else {
                completed.signal()
            }
        }
        DispatchQueue.global().asyncAfter(deadline: .now() + seconds, execute: deadline)

        operation { result in
            completed.wait()
            if !hasCompleted {
                hasCompleted = true
                completed.signal()
                deadline.cancel()
                completion(result)
            } else {
                completed.signal()
            }
        }
    }

    func consumeLastServiceErrorKey(_ completion: @escaping (String?) -> Void) {
        guard let service = service() else {
            completion(nil)
            return
        }
        service.takeLastErrorKey { key in
            DispatchQueue.main.async {
                completion(key)
            }
        }
    }

    func triggerRefresh() {
        logger.info("triggerRefresh called")
        self.lastRefreshTrigger = Date()
        self.isRefreshing = true
        postAccessibilityAnnouncement(L10n.Common.refresh.localized)
        service()?.triggerRefresh { success in
            if !success {
                logger.error("triggerRefresh failed")
                DispatchQueue.main.async {
                    self.isRefreshing = false
                    self.lastRefreshTrigger = nil
                    self.completeOnboardingDetectionProgress()
                    self.postAccessibilityAnnouncement(L10n.Common.error.localized)
                }
            } else {
                DispatchQueue.main.async {
                    self.triggerAvailablePackagesWarmupSearch()
                }
            }
        }
    }

    func triggerOnboardingDetectionRefresh() {
        let visibleMaxTaskId = activeTasks
            .compactMap { UInt64($0.id) }
            .max() ?? 0
        onboardingDetectionAnchorTaskId = max(lastObservedTaskId, visibleMaxTaskId)

        let enabledImplementedManagers = Set(
            ManagerInfo.all
                .filter {
                    let status = managerStatuses[$0.id]
                    let isImplemented = status?.isImplemented ?? $0.isImplemented
                    let isEnabled = status?.enabled ?? true
                    return isImplemented && isEnabled
                }
                .map(\.id)
        )
        onboardingDetectionPendingManagers = enabledImplementedManagers
        onboardingDetectionStartedAt = Date()
        onboardingDetectionInProgress = !enabledImplementedManagers.isEmpty

        triggerRefresh()
    }

    func normalizedManagerName(_ raw: String) -> String {
        switch raw.lowercased() {
        case "homebrew_formula": return L10n.App.Managers.Name.homebrew.localized
        case "homebrew_cask": return L10n.App.Managers.Name.homebrewCask.localized
        case "npm", "npm_global": return L10n.App.Managers.Name.npm.localized
        case "pnpm": return L10n.App.Managers.Name.pnpm.localized
        case "yarn": return L10n.App.Managers.Name.yarn.localized
        case "poetry": return L10n.App.Managers.Name.poetry.localized
        case "rubygems": return L10n.App.Managers.Name.rubygems.localized
        case "bundler": return L10n.App.Managers.Name.bundler.localized
        case "pip": return L10n.App.Managers.Name.pip.localized
        case "pipx": return L10n.App.Managers.Name.pipx.localized
        case "cargo": return L10n.App.Managers.Name.cargo.localized
        case "cargo_binstall": return L10n.App.Managers.Name.cargoBinstall.localized
        case "mise": return L10n.App.Managers.Name.mise.localized
        case "rustup": return L10n.App.Managers.Name.rustup.localized
        case "softwareupdate": return L10n.App.Managers.Name.softwareUpdate.localized
        case "mas": return L10n.App.Managers.Name.appStore.localized
        default: return raw.replacingOccurrences(of: "_", with: " ").capitalized
        }
    }

    func completeOnboarding() {
        UserDefaults.standard.set(true, forKey: "hasCompletedOnboarding")
        hasCompletedOnboarding = true
    }

    func resetDatabase(completion: @escaping (Bool) -> Void) {
        // Stop polling during reset to prevent stale reads
        timer?.invalidate()
        timer = nil

        service()?.resetDatabase { [weak self] success in
            DispatchQueue.main.async {
                if success {
                    self?.installedPackages = []
                    self?.outdatedPackages = []
                    self?.activeTasks = []
                    self?.searchResults = []
                    self?.cachedAvailablePackages = []
                    self?.detectedManagers = []
                    self?.managerStatuses = [:]
                    self?.packageKegPolicyOverrides = [:]
                    self?.homebrewKegAutoCleanupEnabled = false
                    self?.searchText = ""
                    self?.isRefreshing = false
                    self?.onboardingDetectionInProgress = false
                    self?.pinActionPackageIds = []
                    self?.upgradeActionPackageIds = []
                    self?.installActionPackageIds = []
                    self?.uninstallActionPackageIds = []
                    self?.packageDescriptionLoadingIds = []
                    self?.packageDescriptionUnavailableIds = []
                    self?.upgradeActionTaskByPackage = [:]
                    self?.installActionTaskByPackage = [:]
                    self?.uninstallActionTaskByPackage = [:]
                    self?.descriptionLookupTaskIdsByPackage = [:]
                    self?.descriptionLookupLastAttemptByPackage = [:]
                    self?.activeRemoteSearchTaskIds = []
                    self?.lastObservedTaskId = 0
                    self?.onboardingDetectionAnchorTaskId = 0
                    self?.onboardingDetectionPendingManagers = []
                    self?.onboardingDetectionStartedAt = nil
                    self?.lastRefreshTrigger = nil
                    UserDefaults.standard.removeObject(forKey: "hasCompletedOnboarding")
                    self?.hasCompletedOnboarding = false
                }
                // Resume polling after reset
                self?.startPolling()
                completion(success)
            }
        }
    }

    /// Posts a VoiceOver announcement for state changes.
    func postAccessibilityAnnouncement(_ message: String) {
        NSAccessibility.post(
            element: NSApp as Any,
            notification: .announcementRequested,
            userInfo: [
                .announcement: message,
                .priority: NSAccessibilityPriorityLevel.high.rawValue
            ]
        )
    }

    func pruneOnboardingDetectionForDisabledManagers() {
        guard onboardingDetectionInProgress else { return }
        for (managerId, status) in managerStatuses where !status.enabled {
            onboardingDetectionPendingManagers.remove(managerId)
        }
        if onboardingDetectionPendingManagers.isEmpty {
            completeOnboardingDetectionProgress()
        }
    }

    func completeOnboardingDetectionProgress() {
        onboardingDetectionInProgress = false
        onboardingDetectionPendingManagers.removeAll()
        onboardingDetectionStartedAt = nil
    }
}
