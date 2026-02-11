import Foundation

class HelmService: NSObject, HelmServiceProtocol {
    override init() {
        super.init()
        // Initialize Rust Core
        // We need to determine the DB path. For an XPC service, strictly it might be sandboxed.
        // For 0.4.0 alpha, let's assume we pass the path or use a standard one.
        // Ideally the UI passes the path? Or the Service determines it.
        // Let's use the standard Application Support path for the USER.
        
        let appSupport = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        let dbPath = appSupport.appendingPathComponent("Helm/helm.db").path
        
        try? FileManager.default.createDirectory(atPath: appSupport.appendingPathComponent("Helm").path, withIntermediateDirectories: true)
        
        dbPath.withCString { cPath in
            _ = helm_init(cPath)
        }
    }
    
    func listInstalledPackages(withReply reply: @escaping (String?) -> Void) {
        guard let cString = helm_list_installed_packages() else {
            reply(nil)
            return
        }
        defer { helm_free_string(cString) }
        reply(String(cString: cString))
    }
    
    func listTasks(withReply reply: @escaping (String?) -> Void) {
        guard let cString = helm_list_tasks() else {
            reply(nil)
            return
        }
        defer { helm_free_string(cString) }
        reply(String(cString: cString))
    }
    
    func triggerRefresh(withReply reply: @escaping (Bool) -> Void) {
        reply(helm_trigger_refresh())
    }
}
