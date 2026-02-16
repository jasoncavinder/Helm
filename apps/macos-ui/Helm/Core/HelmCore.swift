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
    @Published var detectedManagers: Set<String> = []
    @Published var managerStatuses: [String: ManagerStatus] = [:]
    @Published var managerOperations: [String: String] = [:]
    @Published var pinActionPackageIds: Set<String> = []
    @Published var upgradeActionPackageIds: Set<String> = []
    @Published var onboardingDetectionInProgress: Bool = false
    @Published var homebrewKegAutoCleanupEnabled: Bool = false
    @Published var packageKegPolicyOverrides: [String: HomebrewKegPolicyOverride] = [:]
    @Published var safeModeEnabled: Bool = false
    @Published var selectedManagerFilter: String? = nil
    @Published var hasCompletedOnboarding: Bool = UserDefaults.standard.bool(forKey: "hasCompletedOnboarding")

    private var timer: Timer?
    private var connection: NSXPCConnection?
    private var lastRefreshTrigger: Date?
    private var searchDebounceTimer: Timer?
    private var activeRemoteSearchTaskId: Int64?
    private var managerActionTaskDescriptions: [UInt64: String] = [:]
    private var managerActionTaskByManager: [String: UInt64] = [:]
    private var upgradeActionTaskByPackage: [String: UInt64] = [:]
    private var lastObservedTaskId: UInt64 = 0
    private var onboardingDetectionAnchorTaskId: UInt64 = 0
    private var onboardingDetectionPendingManagers: Set<String> = []
    private var onboardingDetectionStartedAt: Date?

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
        
        if timer == nil {
            startPolling()
        }
        fetchSafeMode()
        fetchHomebrewKegAutoCleanup()
        fetchPackageKegPolicies()
    }

    func scheduleReconnection() {
        DispatchQueue.main.asyncAfter(deadline: .now() + 2.0) { [weak self] in
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

    private func consumeLastServiceErrorKey(_ completion: @escaping (String?) -> Void) {
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
        service()?.triggerRefresh { success in
            if !success {
                logger.error("triggerRefresh failed")
                DispatchQueue.main.async {
                    self.isRefreshing = false
                    self.lastRefreshTrigger = nil
                    self.completeOnboardingDetectionProgress()
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
            ManagerInfo.implemented
                .filter { managerStatuses[$0.id]?.enabled ?? true }
                .map(\.id)
        )
        onboardingDetectionPendingManagers = enabledImplementedManagers
        onboardingDetectionStartedAt = Date()
        onboardingDetectionInProgress = !enabledImplementedManagers.isEmpty

        triggerRefresh()
    }

    private func normalizedManagerName(_ raw: String) -> String {
        switch raw.lowercased() {
        case "homebrew_formula": return "Homebrew"
        case "homebrew_cask": return "Homebrew Cask"
        case "npm_global": return "npm"
        case "pipx": return "pipx"
        case "cargo": return "Cargo"
        case "mise": return "mise"
        case "rustup": return "rustup"
        case "softwareupdate": return "Software Update"
        case "mas": return "App Store"
        default: return raw.replacingOccurrences(of: "_", with: " ").capitalized
        }
    }

    func fetchPackages() {
        service()?.listInstalledPackages { [weak self] jsonString in
            guard let jsonString = jsonString, let data = jsonString.data(using: String.Encoding.utf8) else { return }

            do {
                let decoder = JSONDecoder()
                decoder.keyDecodingStrategy = .convertFromSnakeCase
                let corePackages = try decoder.decode([CoreInstalledPackage].self, from: data)

                DispatchQueue.main.async {
                    self?.installedPackages = corePackages.map { pkg in
                        PackageItem(
                            id: "\(pkg.package.manager):\(pkg.package.name)",
                            name: pkg.package.name,
                            version: pkg.installedVersion ?? L10n.Common.unknown.localized,
                            managerId: pkg.package.manager,
                            manager: self?.normalizedManagerName(pkg.package.manager) ?? pkg.package.manager,
                            pinned: pkg.pinned
                        )
                    }
                }
            } catch {
                logger.error("Failed to decode packages: \(error)")
            }
        }
    }

    func fetchOutdatedPackages() {
        service()?.listOutdatedPackages { [weak self] jsonString in
            guard let jsonString = jsonString, let data = jsonString.data(using: String.Encoding.utf8) else { return }

            do {
                let decoder = JSONDecoder()
                decoder.keyDecodingStrategy = .convertFromSnakeCase
                let corePackages = try decoder.decode([CoreOutdatedPackage].self, from: data)

                DispatchQueue.main.async {
                    self?.outdatedPackages = corePackages.map { pkg in
                        PackageItem(
                            id: "\(pkg.package.manager):\(pkg.package.name)",
                            name: pkg.package.name,
                            version: pkg.installedVersion ?? L10n.Common.unknown.localized,
                            latestVersion: pkg.candidateVersion,
                            managerId: pkg.package.manager,
                            manager: self?.normalizedManagerName(pkg.package.manager) ?? pkg.package.manager,
                            pinned: pkg.pinned,
                            restartRequired: pkg.restartRequired
                        )
                    }
                }
            } catch {
                logger.error("Failed to decode outdated packages: \(error)")
            }
        }
    }

    func fetchTasks() {
        service()?.listTasks { [weak self] jsonString in
            guard let jsonString = jsonString, let data = jsonString.data(using: String.Encoding.utf8) else { return }

            do {
                let decoder = JSONDecoder()
                decoder.keyDecodingStrategy = .convertFromSnakeCase
                let coreTasks = try decoder.decode([CoreTaskRecord].self, from: data)

                DispatchQueue.main.async {
                    if let maxTaskId = coreTasks.map(\.id).max() {
                        self?.lastObservedTaskId = max(self?.lastObservedTaskId ?? 0, maxTaskId)
                    }

                    self?.activeTasks = coreTasks.map { task in
                        let overrideDescription = self?.managerActionTaskDescriptions[task.id]
                        let managerName = self?.normalizedManagerName(task.manager) ?? task.manager
                        let taskLabel = self?.localizedTaskLabel(from: task)
                        return TaskItem(
                            id: "\(task.id)",
                            description: overrideDescription
                                ?? taskLabel
                                ?? L10n.App.Tasks.fallbackDescription.localized(with: [
                                    "task_type": task.taskType.capitalized,
                                    "manager": managerName
                                ]),
                            status: task.status.capitalized
                        )
                    }
                    self?.syncManagerOperations(from: coreTasks)
                    self?.syncUpgradeActions(from: coreTasks)

                    // Derive detection status from Detection-type tasks specifically.
                    // Tasks are ordered most-recent-first. A manager is "detected" if
                    // its latest detection task completed successfully.
                    var latestDetectionByManager: [String: String] = [:]
                    for task in coreTasks {
                        guard task.taskType.lowercased() == "detection" else { continue }
                        if latestDetectionByManager[task.manager] == nil {
                            latestDetectionByManager[task.manager] = task.status.lowercased()
                        }
                    }
                    var detected = Set<String>()
                    for (manager, status) in latestDetectionByManager {
                        if status == "completed" {
                            detected.insert(manager)
                        }
                    }
                    self?.detectedManagers = detected
                    self?.updateOnboardingDetectionProgress(from: coreTasks)

                    let isRunning = coreTasks.contains {
                        let type = $0.taskType.lowercased()
                        let status = $0.status.lowercased()
                        return (type == "refresh" || type == "detection") &&
                               (status == "running" || status == "queued")
                    }

                    // Only show "refreshing" when we triggered a refresh this session.
                    // Without this guard, stale running tasks from a previous session
                    // would permanently lock isRefreshing = true.
                    if let lastTrigger = self?.lastRefreshTrigger {
                        if Date().timeIntervalSince(lastTrigger) > 120.0 {
                            // Safety valve: clear stuck refresh after 2 minutes
                            self?.isRefreshing = false
                            self?.lastRefreshTrigger = nil
                        } else if isRunning {
                            self?.isRefreshing = true
                        } else if Date().timeIntervalSince(lastTrigger) < 2.0 {
                            self?.isRefreshing = true
                        } else {
                            self?.isRefreshing = false
                            self?.lastRefreshTrigger = nil
                        }
                    } else {
                        self?.isRefreshing = false
                    }

                    // Detect remote search completion
                    if let searchTaskId = self?.activeRemoteSearchTaskId {
                        let matchingTask = coreTasks.first { $0.id == UInt64(searchTaskId) }
                        if let task = matchingTask {
                            let status = task.status.lowercased()
                            if status == "completed" || status == "failed" || status == "cancelled" {
                                self?.activeRemoteSearchTaskId = nil
                                self?.isSearching = false
                            }
                        }
                    }
                }
            } catch {
                logger.error("Failed to decode tasks: \(error)")
            }
        }
    }

    func fetchSearchResults(query: String) {
        guard !query.trimmingCharacters(in: .whitespaces).isEmpty else {
            DispatchQueue.main.async {
                self.searchResults = []
            }
            return
        }

        service()?.searchLocal(query: query) { [weak self] jsonString in
            guard let jsonString = jsonString, let data = jsonString.data(using: .utf8) else {
                DispatchQueue.main.async { self?.searchResults = [] }
                return
            }

            do {
                let decoder = JSONDecoder()
                decoder.keyDecodingStrategy = .convertFromSnakeCase
                let results = try decoder.decode([CoreSearchResult].self, from: data)

                DispatchQueue.main.async {
                    self?.searchResults = results.map { result in
                        PackageItem(
                            id: "\(result.sourceManager):\(result.name)",
                            name: result.name,
                            version: result.version ?? "",
                            managerId: result.sourceManager,
                            manager: self?.normalizedManagerName(result.sourceManager) ?? result.sourceManager,
                            summary: result.summary,
                            status: .available
                        )
                    }
                }
            } catch {
                logger.error("Failed to decode search results: \(error)")
                DispatchQueue.main.async { self?.searchResults = [] }
            }
        }
    }

    func refreshCachedAvailablePackages() {
        service()?.searchLocal(query: "") { [weak self] jsonString in
            guard let jsonString = jsonString, let data = jsonString.data(using: .utf8) else { return }

            do {
                let decoder = JSONDecoder()
                decoder.keyDecodingStrategy = .convertFromSnakeCase
                let results = try decoder.decode([CoreSearchResult].self, from: data)

                DispatchQueue.main.async {
                    guard let self = self else { return }
                    let installedIds = Set(self.installedPackages.map { $0.id })
                    self.cachedAvailablePackages = results.compactMap { result in
                        let id = "\(result.sourceManager):\(result.name)"
                        guard !installedIds.contains(id) else { return nil }
                        return PackageItem(
                            id: id,
                            name: result.name,
                            version: result.version ?? "",
                            managerId: result.sourceManager,
                            manager: self.normalizedManagerName(result.sourceManager),
                            summary: result.summary,
                            status: .available
                        )
                    }
                }
            } catch {
                logger.error("Failed to decode cached available packages: \(error)")
            }
        }
    }

    // MARK: - Manager Status

    func fetchManagerStatus() {
        service()?.listManagerStatus { [weak self] jsonString in
            guard let jsonString = jsonString, let data = jsonString.data(using: .utf8) else { return }

            do {
                let decoder = JSONDecoder()
                let statuses = try decoder.decode([ManagerStatus].self, from: data)

                DispatchQueue.main.async {
                    var map: [String: ManagerStatus] = [:]
                    for status in statuses {
                        map[status.managerId] = status
                    }
                    self?.managerStatuses = map
                    self?.pruneOnboardingDetectionForDisabledManagers()
                }
            } catch {
                logger.error("Failed to decode manager statuses: \(error)")
            }
        }
    }

    func fetchSafeMode() {
        service()?.getSafeMode { [weak self] enabled in
            DispatchQueue.main.async {
                self?.safeModeEnabled = enabled
            }
        }
    }

    func setSafeMode(_ enabled: Bool) {
        service()?.setSafeMode(enabled: enabled) { [weak self] success in
            DispatchQueue.main.async {
                if success {
                    self?.safeModeEnabled = enabled
                } else {
                    logger.error("setSafeMode(\(enabled)) failed")
                }
            }
        }
    }

    func fetchHomebrewKegAutoCleanup() {
        service()?.getHomebrewKegAutoCleanup { [weak self] enabled in
            DispatchQueue.main.async {
                self?.homebrewKegAutoCleanupEnabled = enabled
            }
        }
    }

    func setHomebrewKegAutoCleanup(_ enabled: Bool) {
        service()?.setHomebrewKegAutoCleanup(enabled: enabled) { [weak self] success in
            DispatchQueue.main.async {
                if success {
                    self?.homebrewKegAutoCleanupEnabled = enabled
                } else {
                    logger.error("setHomebrewKegAutoCleanup(\(enabled)) failed")
                }
            }
        }
    }

    func fetchPackageKegPolicies() {
        service()?.listPackageKegPolicies { [weak self] jsonString in
            guard let jsonString = jsonString, let data = jsonString.data(using: .utf8) else { return }

            do {
                let decoder = JSONDecoder()
                decoder.keyDecodingStrategy = .convertFromSnakeCase
                let entries = try decoder.decode([CorePackageKegPolicy].self, from: data)

                DispatchQueue.main.async {
                    var overrides: [String: HomebrewKegPolicyOverride] = [:]
                    for entry in entries where entry.managerId == "homebrew_formula" {
                        overrides["\(entry.managerId):\(entry.packageName)"] = entry.policy
                    }
                    self?.packageKegPolicyOverrides = overrides
                }
            } catch {
                logger.error("Failed to decode package keg policies: \(error)")
            }
        }
    }

    func upgradeAll(includePinned: Bool = false, allowOsUpdates: Bool = false) {
        service()?.upgradeAll(includePinned: includePinned, allowOsUpdates: allowOsUpdates) { success in
            if !success {
                logger.error("upgradeAll(includePinned: \(includePinned), allowOsUpdates: \(allowOsUpdates)) failed")
            }
        }
    }

    func upgradeAllPreviewCount(includePinned: Bool = false, allowOsUpdates: Bool = false) -> Int {
        outdatedPackages.filter { package in
            guard includePinned || !package.pinned else { return false }
            guard managerStatuses[package.managerId]?.enabled ?? true else { return false }
            if package.managerId == "softwareupdate" && !allowOsUpdates {
                return false
            }
            if package.managerId == "softwareupdate" && safeModeEnabled {
                return false
            }
            return true
        }.count
    }

    func upgradeAllPreviewBreakdown(
        includePinned: Bool = false,
        allowOsUpdates: Bool = false
    ) -> [(manager: String, count: Int)] {
        var counts: [String: Int] = [:]

        for package in outdatedPackages {
            guard includePinned || !package.pinned else { continue }
            guard managerStatuses[package.managerId]?.enabled ?? true else { continue }
            if package.managerId == "softwareupdate" && !allowOsUpdates {
                continue
            }
            if package.managerId == "softwareupdate" && safeModeEnabled {
                continue
            }

            let manager = normalizedManagerName(package.managerId)
            counts[manager, default: 0] += 1
        }

        return counts
            .map { (manager: $0.key, count: $0.value) }
            .sorted { lhs, rhs in
                if lhs.count == rhs.count {
                    return lhs.manager.localizedCaseInsensitiveCompare(rhs.manager) == .orderedAscending
                }
                return lhs.count > rhs.count
            }
    }

    func kegPolicySelection(for package: PackageItem) -> KegPolicySelection {
        guard package.managerId == "homebrew_formula" else { return .useGlobal }

        switch packageKegPolicyOverrides[package.id] {
        case .keep:
            return .keep
        case .cleanup:
            return .cleanup
        case .none:
            return .useGlobal
        }
    }

    func setKegPolicySelection(for package: PackageItem, selection: KegPolicySelection) {
        guard package.managerId == "homebrew_formula" else { return }

        let policyMode: Int32
        switch selection {
        case .useGlobal:
            policyMode = -1
        case .keep:
            policyMode = 0
        case .cleanup:
            policyMode = 1
        }

        service()?.setPackageKegPolicy(managerId: package.managerId, packageName: package.name, policyMode: policyMode) { [weak self] success in
            DispatchQueue.main.async {
                guard let self = self else { return }
                guard success else {
                    logger.error("setPackageKegPolicy(\(package.managerId):\(package.name), \(policyMode)) failed")
                    return
                }
                switch selection {
                case .useGlobal:
                    self.packageKegPolicyOverrides.removeValue(forKey: package.id)
                case .keep:
                    self.packageKegPolicyOverrides[package.id] = .keep
                case .cleanup:
                    self.packageKegPolicyOverrides[package.id] = .cleanup
                }
            }
        }
    }

    func canUpgradeIndividually(_ package: PackageItem) -> Bool {
        let upgradableManagers: Set<String> = ["homebrew_formula", "mise", "rustup"]
        return package.status == .upgradable
            && upgradableManagers.contains(package.managerId)
            && !package.pinned
    }

    func upgradePackage(_ package: PackageItem) {
        guard canUpgradeIndividually(package) else { return }

        DispatchQueue.main.async {
            self.upgradeActionPackageIds.insert(package.id)
        }

        guard let service = service() else {
            logger.error("upgradePackage(\(package.managerId):\(package.name)) failed: service unavailable")
            DispatchQueue.main.async {
                self.upgradeActionPackageIds.remove(package.id)
            }
            return
        }

        service.upgradePackage(managerId: package.managerId, packageName: package.name) { [weak self] taskId in
            DispatchQueue.main.async {
                guard let self = self else { return }
                if taskId < 0 {
                    self.upgradeActionTaskByPackage.removeValue(forKey: package.id)
                    self.upgradeActionPackageIds.remove(package.id)
                    logger.error("upgradePackage(\(package.managerId):\(package.name)) failed")
                    return
                }

                self.upgradeActionTaskByPackage[package.id] = UInt64(taskId)
                self.registerManagerActionTask(
                    managerId: package.managerId,
                    taskId: UInt64(taskId),
                    description: self.upgradeActionDescription(for: package),
                    inProgressText: L10n.App.Managers.Operation.upgrading.localized
                )
            }
        }
    }

    func setManagerEnabled(_ managerId: String, enabled: Bool) {
        service()?.setManagerEnabled(managerId: managerId, enabled: enabled) { success in
            if !success {
                logger.error("setManagerEnabled(\(managerId), \(enabled)) failed")
            }
        }
    }

    func installManager(_ managerId: String) {
        DispatchQueue.main.async {
            self.managerOperations[managerId] = L10n.App.Managers.Operation.startingInstall.localized
        }
        service()?.installManager(managerId: managerId) { [weak self] taskId in
            DispatchQueue.main.async {
                guard let self = self else { return }
                if taskId < 0 {
                    logger.error("installManager(\(managerId)) failed")
                    self.consumeLastServiceErrorKey { serviceErrorKey in
                        self.managerOperations[managerId] =
                            serviceErrorKey?.localized ?? L10n.App.Managers.Operation.installFailed.localized
                    }
                    return
                }
                self.registerManagerActionTask(
                    managerId: managerId,
                    taskId: UInt64(taskId),
                    description: self.managerActionDescription(action: "Install", managerId: managerId),
                    inProgressText: L10n.App.Managers.Operation.installing.localized
                )
            }
        }
    }

    func updateManager(_ managerId: String) {
        DispatchQueue.main.async {
            self.managerOperations[managerId] = L10n.App.Managers.Operation.startingUpdate.localized
        }
        service()?.updateManager(managerId: managerId) { [weak self] taskId in
            DispatchQueue.main.async {
                guard let self = self else { return }
                if taskId < 0 {
                    logger.error("updateManager(\(managerId)) failed")
                    self.consumeLastServiceErrorKey { serviceErrorKey in
                        self.managerOperations[managerId] =
                            serviceErrorKey?.localized ?? L10n.App.Managers.Operation.updateFailed.localized
                    }
                    return
                }
                self.registerManagerActionTask(
                    managerId: managerId,
                    taskId: UInt64(taskId),
                    description: self.managerActionDescription(action: "Update", managerId: managerId),
                    inProgressText: L10n.App.Managers.Operation.updating.localized
                )
            }
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
                    self?.upgradeActionTaskByPackage = [:]
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

    func uninstallManager(_ managerId: String) {
        DispatchQueue.main.async {
            self.managerOperations[managerId] = L10n.App.Managers.Operation.startingUninstall.localized
        }
        service()?.uninstallManager(managerId: managerId) { [weak self] taskId in
            DispatchQueue.main.async {
                guard let self = self else { return }
                if taskId < 0 {
                    logger.error("uninstallManager(\(managerId)) failed")
                    self.consumeLastServiceErrorKey { serviceErrorKey in
                        self.managerOperations[managerId] =
                            serviceErrorKey?.localized ?? L10n.App.Managers.Operation.uninstallFailed.localized
                    }
                    return
                }
                self.registerManagerActionTask(
                    managerId: managerId,
                    taskId: UInt64(taskId),
                    description: self.managerActionDescription(action: "Uninstall", managerId: managerId),
                    inProgressText: L10n.App.Managers.Operation.uninstalling.localized
                )
            }
        }
    }

    func pinPackage(_ package: PackageItem) {
        DispatchQueue.main.async {
            self.pinActionPackageIds.insert(package.id)
        }
        guard let service = service() else {
            logger.error("pinPackage(\(package.managerId):\(package.name)) failed: service unavailable")
            DispatchQueue.main.async {
                self.pinActionPackageIds.remove(package.id)
            }
            return
        }
        let version = package.version.isEmpty || package.version == "unknown" ? nil : package.version
        service.pinPackage(managerId: package.managerId, packageName: package.name, version: version) { [weak self] success in
            DispatchQueue.main.async {
                self?.pinActionPackageIds.remove(package.id)
                if success {
                    self?.setPinnedState(packageId: package.id, pinned: true)
                    self?.fetchPackages()
                    self?.fetchOutdatedPackages()
                } else {
                    logger.error("pinPackage(\(package.managerId):\(package.name)) failed")
                }
            }
        }
    }

    func unpinPackage(_ package: PackageItem) {
        DispatchQueue.main.async {
            self.pinActionPackageIds.insert(package.id)
        }
        guard let service = service() else {
            logger.error("unpinPackage(\(package.managerId):\(package.name)) failed: service unavailable")
            DispatchQueue.main.async {
                self.pinActionPackageIds.remove(package.id)
            }
            return
        }
        service.unpinPackage(managerId: package.managerId, packageName: package.name) { [weak self] success in
            DispatchQueue.main.async {
                self?.pinActionPackageIds.remove(package.id)
                if success {
                    self?.setPinnedState(packageId: package.id, pinned: false)
                    self?.fetchPackages()
                    self?.fetchOutdatedPackages()
                } else {
                    logger.error("unpinPackage(\(package.managerId):\(package.name)) failed")
                }
            }
        }
    }

    // MARK: - Search Orchestration

    private func onSearchTextChanged(_ query: String) {
        // 1. Instant local cache query
        fetchSearchResults(query: query)

        // 2. Cancel in-flight remote search
        cancelActiveRemoteSearch()

        // 3. Invalidate debounce timer
        searchDebounceTimer?.invalidate()
        searchDebounceTimer = nil

        // 4. If empty, clear state and return
        guard !query.trimmingCharacters(in: .whitespaces).isEmpty else {
            isSearching = false
            return
        }

        // 5. Start 300ms debounce timer for remote search
        searchDebounceTimer = Timer.scheduledTimer(withTimeInterval: 0.3, repeats: false) { [weak self] _ in
            self?.triggerRemoteSearch(query: query)
        }
    }

    private func triggerRemoteSearch(query: String) {
        isSearching = true
        service()?.triggerRemoteSearch(query: query) { [weak self] taskId in
            DispatchQueue.main.async {
                if taskId >= 0 {
                    self?.activeRemoteSearchTaskId = taskId
                } else {
                    logger.error("triggerRemoteSearch returned error")
                    self?.isSearching = false
                }
            }
        }
    }

    private func cancelActiveRemoteSearch() {
        guard let taskId = activeRemoteSearchTaskId else { return }
        activeRemoteSearchTaskId = nil
        isSearching = false
        service()?.cancelTask(taskId: taskId) { success in
            if !success {
                logger.warning("cancelTask(\(taskId)) returned false")
            }
        }
    }

    private func clearSearchState() {
        activeRemoteSearchTaskId = nil
        isSearching = false
    }

    private func setPinnedState(packageId: String, pinned: Bool) {
        if let index = installedPackages.firstIndex(where: { $0.id == packageId }) {
            installedPackages[index].pinned = pinned
        }
        if let index = outdatedPackages.firstIndex(where: { $0.id == packageId }) {
            outdatedPackages[index].pinned = pinned
        }
        if let index = searchResults.firstIndex(where: { $0.id == packageId }) {
            searchResults[index].pinned = pinned
        }
        if let index = cachedAvailablePackages.firstIndex(where: { $0.id == packageId }) {
            cachedAvailablePackages[index].pinned = pinned
        }
    }

    private func registerManagerActionTask(
        managerId: String,
        taskId: UInt64,
        description: String,
        inProgressText: String
    ) {
        managerActionTaskDescriptions[taskId] = description
        managerActionTaskByManager[managerId] = taskId
        managerOperations[managerId] = inProgressText

        let idString = "\(taskId)"
        if !activeTasks.contains(where: { $0.id == idString }) {
            activeTasks.insert(
                TaskItem(
                    id: idString,
                    description: description,
                    status: "Queued"
                ),
                at: 0
            )
        }
    }

    private func syncManagerOperations(from coreTasks: [CoreTaskRecord]) {
        let statusById = Dictionary(uniqueKeysWithValues: coreTasks.map { ($0.id, $0.status.lowercased()) })
        let inFlightStates = Set(["queued", "running"])

        for managerId in Array(managerActionTaskByManager.keys) {
            guard let taskId = managerActionTaskByManager[managerId] else { continue }
            guard let status = statusById[taskId] else {
                managerOperations.removeValue(forKey: managerId)
                managerActionTaskByManager.removeValue(forKey: managerId)
                continue
            }
            if !inFlightStates.contains(status) {
                managerOperations.removeValue(forKey: managerId)
                managerActionTaskByManager.removeValue(forKey: managerId)
            }
        }
    }

    private func syncUpgradeActions(from coreTasks: [CoreTaskRecord]) {
        let statusById = Dictionary(uniqueKeysWithValues: coreTasks.map { ($0.id, $0.status.lowercased()) })
        let inFlightStates = Set(["queued", "running"])
        var shouldRefreshSnapshots = false

        for packageId in Array(upgradeActionTaskByPackage.keys) {
            guard let taskId = upgradeActionTaskByPackage[packageId] else { continue }
            guard let status = statusById[taskId] else {
                upgradeActionTaskByPackage.removeValue(forKey: packageId)
                upgradeActionPackageIds.remove(packageId)
                continue
            }
            if inFlightStates.contains(status) {
                continue
            }

            upgradeActionTaskByPackage.removeValue(forKey: packageId)
            upgradeActionPackageIds.remove(packageId)
            if status == "completed" {
                shouldRefreshSnapshots = true
            }
        }

        if shouldRefreshSnapshots {
            fetchPackages()
            fetchOutdatedPackages()
        }
    }

    private func updateOnboardingDetectionProgress(from coreTasks: [CoreTaskRecord]) {
        guard onboardingDetectionInProgress else { return }

        let terminalStatuses = Set(["completed", "failed", "cancelled"])
        for task in coreTasks
        where task.id > onboardingDetectionAnchorTaskId
            && task.taskType.lowercased() == "detection"
            && terminalStatuses.contains(task.status.lowercased())
        {
            onboardingDetectionPendingManagers.remove(task.manager)
        }

        pruneOnboardingDetectionForDisabledManagers()

        if onboardingDetectionPendingManagers.isEmpty {
            completeOnboardingDetectionProgress()
            return
        }

        if let startedAt = onboardingDetectionStartedAt,
           Date().timeIntervalSince(startedAt) > 90
        {
            let pending = onboardingDetectionPendingManagers.joined(separator: ",")
            logger.warning("Onboarding detection timed out waiting for managers: \(pending)")
            completeOnboardingDetectionProgress()
        }
    }

    private func pruneOnboardingDetectionForDisabledManagers() {
        guard onboardingDetectionInProgress else { return }
        for (managerId, status) in managerStatuses where !status.enabled {
            onboardingDetectionPendingManagers.remove(managerId)
        }
        if onboardingDetectionPendingManagers.isEmpty {
            completeOnboardingDetectionProgress()
        }
    }

    private func completeOnboardingDetectionProgress() {
        onboardingDetectionInProgress = false
        onboardingDetectionPendingManagers.removeAll()
        onboardingDetectionStartedAt = nil
    }

    private func shouldCleanupOldKegs(for package: PackageItem) -> Bool {
        if package.managerId != "homebrew_formula" {
            return false
        }

        switch kegPolicySelection(for: package) {
        case .cleanup:
            return true
        case .keep:
            return false
        case .useGlobal:
            return homebrewKegAutoCleanupEnabled
        }
    }

    private func localizedTaskLabel(from task: CoreTaskRecord) -> String? {
        if let labelKey = task.labelKey {
            let args = task.labelArgs?.reduce(into: [String: Any]()) { partialResult, entry in
                partialResult[entry.key] = entry.value
            } ?? [:]
            return labelKey.localized(with: args)
        }
        return task.label
    }

    private func upgradeActionDescription(for package: PackageItem) -> String {
        switch package.managerId {
        case "homebrew_formula":
            if shouldCleanupOldKegs(for: package) {
                return L10n.Service.Task.Label.upgradeHomebrewCleanup.localized(with: ["package": package.name])
            }
            return L10n.Service.Task.Label.upgradeHomebrew.localized(with: ["package": package.name])
        case "mise":
            return L10n.Service.Task.Label.upgradeMise.localized(with: ["package": package.name])
        case "rustup":
            return L10n.Service.Task.Label.upgradeRustupToolchain.localized(with: ["toolchain": package.name])
        default:
            return L10n.App.Tasks.fallbackDescription.localized(with: [
                "task_type": L10n.Common.update.localized,
                "manager": normalizedManagerName(package.managerId)
            ])
        }
    }

    private func managerActionDescription(action: String, managerId: String) -> String {
        switch (action, managerId) {
        case ("Install", "mas"):
            return L10n.Service.Task.Label.installHomebrewFormula.localized(with: ["package": "mas"])
        case ("Install", "mise"):
            return L10n.Service.Task.Label.installHomebrewFormula.localized(with: ["package": "mise"])
        case ("Update", "homebrew_formula"):
            return L10n.Service.Task.Label.updateHomebrewSelf.localized
        case ("Update", "mas"):
            return L10n.Service.Task.Label.updateHomebrewFormula.localized(with: ["package": "mas"])
        case ("Update", "mise"):
            return L10n.Service.Task.Label.updateHomebrewFormula.localized(with: ["package": "mise"])
        case ("Update", "rustup"):
            return L10n.Service.Task.Label.updateRustupSelf.localized
        case ("Uninstall", "mas"):
            return L10n.Service.Task.Label.uninstallHomebrewFormula.localized(with: ["package": "mas"])
        case ("Uninstall", "mise"):
            return L10n.Service.Task.Label.uninstallHomebrewFormula.localized(with: ["package": "mise"])
        case ("Uninstall", "rustup"):
            return L10n.Service.Task.Label.uninstallRustupSelf.localized
        default:
            return L10n.App.Tasks.fallbackDescription.localized(with: [
                "task_type": action,
                "manager": normalizedManagerName(managerId)
            ])
        }
    }
}
