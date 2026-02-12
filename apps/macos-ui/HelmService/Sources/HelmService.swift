import Foundation
import os.log

private let logger = Logger(subsystem: "app.jasoncavinder.Helm.HelmService", category: "service")

class HelmService: NSObject, HelmServiceProtocol {
    override init() {
        super.init()

        let appSupport = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        let dbPath = appSupport.appendingPathComponent("Helm/helm.db").path

        logger.info("HelmService init — DB path: \(dbPath)")

        try? FileManager.default.createDirectory(atPath: appSupport.appendingPathComponent("Helm").path, withIntermediateDirectories: true)

        let result = dbPath.withCString { cPath in
            helm_init(cPath)
        }
        logger.info("helm_init result: \(result)")
    }

    func listInstalledPackages(withReply reply: @escaping (String?) -> Void) {
        guard let cString = helm_list_installed_packages() else {
            logger.warning("helm_list_installed_packages returned nil")
            reply(nil)
            return
        }
        defer { helm_free_string(cString) }
        let json = String(cString: cString)
        logger.info("listInstalledPackages: \(json.prefix(200))")
        reply(json)
    }

    func listTasks(withReply reply: @escaping (String?) -> Void) {
        guard let cString = helm_list_tasks() else {
            logger.warning("helm_list_tasks returned nil")
            reply(nil)
            return
        }
        defer { helm_free_string(cString) }
        let json = String(cString: cString)
        logger.info("listTasks: \(json.prefix(200))")
        reply(json)
    }

    func triggerRefresh(withReply reply: @escaping (Bool) -> Void) {
        let result = helm_trigger_refresh()
        logger.info("helm_trigger_refresh result: \(result)")
        reply(result)
    }
}
