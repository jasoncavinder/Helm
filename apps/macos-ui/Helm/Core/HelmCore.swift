import Foundation

final class HelmCore: ObservableObject {
    static let shared = HelmCore()
    
    @Published var isInitialized = false
    
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
        } else {
            print("Failed to initialize Helm Core")
        }
    }
}
