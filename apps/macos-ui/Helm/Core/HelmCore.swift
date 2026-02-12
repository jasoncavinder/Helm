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

final class HelmCore: ObservableObject {
    static let shared = HelmCore()

    @Published var isInitialized = false
    @Published var isConnected = false
    @Published var isRefreshing = false
    @Published var installedPackages: [PackageItem] = []
    @Published var outdatedPackages: [PackageItem] = []
    @Published var activeTasks: [TaskItem] = []

    private var timer: Timer?
    private var connection: NSXPCConnection?
    private var lastRefreshTrigger: Date?

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
                self?.scheduleReconnection()
            }
        }
        connection.interruptionHandler = { [weak self] in
            logger.error("XPC connection interrupted")
            DispatchQueue.main.async {
                self?.isConnected = false
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
                            manager: pkg.package.manager.replacingOccurrences(of: "_", with: " ").capitalized
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
                            manager: pkg.package.manager.replacingOccurrences(of: "_", with: " ").capitalized
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
                        TaskItem(
                            id: "\(task.id)",
                            description: "\(task.taskType.capitalized) \(task.manager.replacingOccurrences(of: "_", with: " ").capitalized)",
                            status: task.status.capitalized
                        )
                    }

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
                }
            } catch {
                logger.error("Failed to decode tasks: \(error)")
            }
        }
    }
}
