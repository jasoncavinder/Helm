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
}

struct CoreSearchResult: Codable {
    let manager: String
    let name: String
    let version: String?
    let summary: String?
    let sourceManager: String
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
                }
            }
        }
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
                            version: pkg.installedVersion ?? "unknown",
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
                            version: pkg.installedVersion ?? "unknown",
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
                    self?.activeTasks = coreTasks.map { task in
                        let overrideDescription = self?.managerActionTaskDescriptions[task.id]
                        let managerName = self?.normalizedManagerName(task.manager) ?? task.manager
                        return TaskItem(
                            id: "\(task.id)",
                            description: overrideDescription ?? task.label ?? "\(task.taskType.capitalized) \(managerName)",
                            status: task.status.capitalized
                        )
                    }
                    self?.syncManagerOperations(from: coreTasks)

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

    func upgradeAll(includePinned: Bool = false, allowOsUpdates: Bool = false) {
        service()?.upgradeAll(includePinned: includePinned, allowOsUpdates: allowOsUpdates) { success in
            if !success {
                logger.error("upgradeAll(includePinned: \(includePinned), allowOsUpdates: \(allowOsUpdates)) failed")
            }
        }
    }

    func canUpgradeIndividually(_ package: PackageItem) -> Bool {
        package.status == .upgradable && package.managerId == "homebrew_formula" && !package.pinned
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
                self.upgradeActionPackageIds.remove(package.id)

                if taskId < 0 {
                    logger.error("upgradePackage(\(package.managerId):\(package.name)) failed")
                    return
                }

                self.registerManagerActionTask(
                    managerId: package.managerId,
                    taskId: UInt64(taskId),
                    description: "Upgrade \(package.name) via \(self.normalizedManagerName(package.managerId))",
                    inProgressText: "Upgrading..."
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
            self.managerOperations[managerId] = "Starting install..."
        }
        service()?.installManager(managerId: managerId) { [weak self] taskId in
            DispatchQueue.main.async {
                guard let self = self else { return }
                if taskId < 0 {
                    logger.error("installManager(\(managerId)) failed")
                    self.managerOperations[managerId] = "Install failed"
                    return
                }
                self.registerManagerActionTask(
                    managerId: managerId,
                    taskId: UInt64(taskId),
                    description: self.managerActionDescription(action: "Install", managerId: managerId),
                    inProgressText: "Installing..."
                )
            }
        }
    }

    func updateManager(_ managerId: String) {
        DispatchQueue.main.async {
            self.managerOperations[managerId] = "Starting update..."
        }
        service()?.updateManager(managerId: managerId) { [weak self] taskId in
            DispatchQueue.main.async {
                guard let self = self else { return }
                if taskId < 0 {
                    logger.error("updateManager(\(managerId)) failed")
                    self.managerOperations[managerId] = "Update failed"
                    return
                }
                self.registerManagerActionTask(
                    managerId: managerId,
                    taskId: UInt64(taskId),
                    description: self.managerActionDescription(action: "Update", managerId: managerId),
                    inProgressText: "Updating..."
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
                    self?.searchText = ""
                    self?.isRefreshing = false
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
            self.managerOperations[managerId] = "Starting uninstall..."
        }
        service()?.uninstallManager(managerId: managerId) { [weak self] taskId in
            DispatchQueue.main.async {
                guard let self = self else { return }
                if taskId < 0 {
                    logger.error("uninstallManager(\(managerId)) failed")
                    self.managerOperations[managerId] = "Uninstall failed"
                    return
                }
                self.registerManagerActionTask(
                    managerId: managerId,
                    taskId: UInt64(taskId),
                    description: self.managerActionDescription(action: "Uninstall", managerId: managerId),
                    inProgressText: "Uninstalling..."
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

    private func managerActionDescription(action: String, managerId: String) -> String {
        switch (action, managerId) {
        case ("Install", "mas"):
            return "Install mas (Mac App Store manager) via Homebrew"
        case ("Install", "mise"):
            return "Install mise via Homebrew"
        case ("Update", "homebrew_formula"):
            return "Update Homebrew"
        case ("Update", "mas"):
            return "Update mas via Homebrew"
        case ("Update", "mise"):
            return "Update mise via Homebrew"
        case ("Update", "rustup"):
            return "Self-update rustup"
        case ("Uninstall", "mas"):
            return "Uninstall mas via Homebrew"
        case ("Uninstall", "mise"):
            return "Uninstall mise via Homebrew"
        default:
            return "\(action) \(normalizedManagerName(managerId))"
        }
    }
}
