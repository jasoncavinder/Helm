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
}

struct CoreTaskRecord: Codable {
    let id: UInt64
    let manager: String
    let taskType: String
    let status: String
}

struct CoreSearchResult: Codable {
    let manager: String
    let name: String
    let version: String?
    let summary: String?
    let sourceManager: String
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

    private var timer: Timer?
    private var connection: NSXPCConnection?
    private var lastRefreshTrigger: Date?
    private var searchDebounceTimer: Timer?
    private var activeRemoteSearchTaskId: Int64?

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
        DispatchQueue.main.async {
            self.lastRefreshTrigger = Date()
            self.isRefreshing = true
        }
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
        case "mas": return "App Store"
        default: return raw.replacingOccurrences(of: "_", with: " ").capitalized
        }
    }

    func fetchPackages() {
        service()?.listInstalledPackages { [weak self] jsonString in
            guard let jsonString = jsonString, let data = jsonString.data(using: .utf8) else { return }

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
                            manager: self?.normalizedManagerName(pkg.package.manager) ?? pkg.package.manager
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
            guard let jsonString = jsonString, let data = jsonString.data(using: .utf8) else { return }

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
                            manager: self?.normalizedManagerName(pkg.package.manager) ?? pkg.package.manager
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
            guard let jsonString = jsonString, let data = jsonString.data(using: .utf8) else { return }

            do {
                let decoder = JSONDecoder()
                decoder.keyDecodingStrategy = .convertFromSnakeCase
                let coreTasks = try decoder.decode([CoreTaskRecord].self, from: data)

                DispatchQueue.main.async {
                    self?.activeTasks = coreTasks.map { task in
                        let managerName = self?.normalizedManagerName(task.manager) ?? task.manager
                        return TaskItem(
                            id: "\(task.id)",
                            description: "\(task.taskType.capitalized) \(managerName)",
                            status: task.status.capitalized
                        )
                    }

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
                        $0.taskType.lowercased() == "refresh" &&
                        ($0.status.lowercased() == "running" || $0.status.lowercased() == "queued")
                    }

                    if isRunning {
                        self?.isRefreshing = true
                        self?.lastRefreshTrigger = nil
                    } else if let lastTrigger = self?.lastRefreshTrigger, Date().timeIntervalSince(lastTrigger) < 2.0 {
                        self?.isRefreshing = true
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
}
