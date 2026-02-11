import Foundation

struct CorePackageRef: Codable {
    let manager: String
    let name: String
}

struct CoreInstalledPackage: Codable {
    let package: CorePackageRef
    let installed_version: String?
    let pinned: Bool
}

struct CoreTaskRecord: Codable {
    let id: UInt64
    let manager: String
    let task_type: String
    let status: String
}

final class HelmCore: ObservableObject {
    static let shared = HelmCore()
    
    @Published var isInitialized = false
    @Published var installedPackages: [PackageItem] = []
    @Published var activeTasks: [TaskItem] = []
    
    private var timer: Timer?
    private var connection: NSXPCConnection?
    
    private init() {
        setupConnection()
    }
    
    func setupConnection() {
        let connection = NSXPCConnection(serviceName: "com.jasoncavinder.Helm.HelmService")
        connection.remoteObjectInterface = NSXPCInterface(with: HelmServiceProtocol.self)
        connection.resume()
        self.connection = connection
        
        isInitialized = true
        startPolling()
    }
    
    func startPolling() {
        timer = Timer.scheduledTimer(withTimeInterval: 1.0, repeats: true) { [weak self] _ in
            self?.fetchTasks()
            self?.fetchPackages()
        }
    }
    
    func service() -> HelmServiceProtocol? {
        return connection?.remoteObjectProxy as? HelmServiceProtocol
    }
    
    func triggerRefresh() {
        service()?.triggerRefresh { success in
            if success {
                print("Refresh triggered via XPC")
            } else {
                print("Failed to trigger refresh via XPC")
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
                            version: pkg.installed_version ?? "unknown",
                            manager: pkg.package.manager.replacingOccurrences(of: "_", with: " ").capitalized
                        )
                    }
                }
            } catch {
                print("Failed to decode packages: \(error)")
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
                            description: "\(task.task_type.capitalized) \(task.manager.replacingOccurrences(of: "_", with: " ").capitalized)",
                            status: task.status.capitalized
                        )
                    }
                }
            } catch {
                print("Failed to decode tasks: \(error)")
            }
        }
    }
}
