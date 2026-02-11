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
    
    private init() {
        setup()
    }
    
    func setup() {
        // Path to SQLite DB in Application Support
        let appSupport = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        let dbPath = appSupport.appendingPathComponent("Helm/helm.db").path
        
        // Ensure directory exists
        try? FileManager.default.createDirectory(atPath: appSupport.appendingPathComponent("Helm").path, withIntermediateDirectories: true)
        
        // Call Rust init
        let success = dbPath.withCString { cPath in
            helm_init(cPath)
        }
        
        if success {
            print("Helm Core initialized successfully")
            isInitialized = true
            fetchPackages()
            startPolling()
        } else {
            print("Failed to initialize Helm Core")
        }
    }
    
    func startPolling() {
        timer = Timer.scheduledTimer(withTimeInterval: 1.0, repeats: true) { [weak self] _ in
            self?.fetchTasks()
            self?.fetchPackages()
        }
    }
    
    func triggerRefresh() {
        if helm_trigger_refresh() {
            print("Refresh triggered")
        } else {
            print("Failed to trigger refresh")
        }
    }
    
    func fetchPackages() {
        guard let cString = helm_list_installed_packages() else {
            return
        }
        defer { helm_free_string(cString) }
        
        let jsonString = String(cString: cString)
        guard let data = jsonString.data(using: .utf8) else { return }
        
        do {
            let decoder = JSONDecoder()
            decoder.keyDecodingStrategy = .convertFromSnakeCase
            let corePackages = try decoder.decode([CoreInstalledPackage].self, from: data)
            
            DispatchQueue.main.async {
                self.installedPackages = corePackages.map { pkg in
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
    
    func fetchTasks() {
        guard let cString = helm_list_tasks() else {
            return
        }
        defer { helm_free_string(cString) }
        
        let jsonString = String(cString: cString)
        guard let data = jsonString.data(using: .utf8) else { return }
        
        do {
            let decoder = JSONDecoder()
            decoder.keyDecodingStrategy = .convertFromSnakeCase
            let coreTasks = try decoder.decode([CoreTaskRecord].self, from: data)
            
            DispatchQueue.main.async {
                self.activeTasks = coreTasks.map { task in
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
