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
        reply(String(cString: cString))
    }

    func listOutdatedPackages(withReply reply: @escaping (String?) -> Void) {
        guard let cString = helm_list_outdated_packages() else {
            logger.warning("helm_list_outdated_packages returned nil")
            reply(nil)
            return
        }
        defer { helm_free_string(cString) }
        reply(String(cString: cString))
    }

    func listTasks(withReply reply: @escaping (String?) -> Void) {
        guard let cString = helm_list_tasks() else {
            logger.warning("helm_list_tasks returned nil")
            reply(nil)
            return
        }
        defer { helm_free_string(cString) }
        reply(String(cString: cString))
    }

    func triggerRefresh(withReply reply: @escaping (Bool) -> Void) {
        let result = helm_trigger_refresh()
        logger.info("helm_trigger_refresh result: \(result)")
        reply(result)
    }

    func searchLocal(query: String, withReply reply: @escaping (String?) -> Void) {
        guard let cString = query.withCString({ helm_search_local($0) }) else {
            logger.warning("helm_search_local returned nil")
            reply(nil)
            return
        }
        defer { helm_free_string(cString) }
        reply(String(cString: cString))
    }

    func triggerRemoteSearch(query: String, withReply reply: @escaping (Int64) -> Void) {
        let taskId = query.withCString { helm_trigger_remote_search($0) }
        logger.info("helm_trigger_remote_search result: \(taskId)")
        reply(taskId)
    }

    func cancelTask(taskId: Int64, withReply reply: @escaping (Bool) -> Void) {
        let result = helm_cancel_task(taskId)
        logger.info("helm_cancel_task(\(taskId)) result: \(result)")
        reply(result)
    }

    func listManagerStatus(withReply reply: @escaping (String?) -> Void) {
        guard let cString = helm_list_manager_status() else {
            logger.warning("helm_list_manager_status returned nil")
            reply(nil)
            return
        }
        defer { helm_free_string(cString) }
        reply(String(cString: cString))
    }

    func setManagerEnabled(managerId: String, enabled: Bool, withReply reply: @escaping (Bool) -> Void) {
        let result = managerId.withCString { helm_set_manager_enabled($0, enabled) }
        logger.info("helm_set_manager_enabled(\(managerId), \(enabled)) result: \(result)")
        reply(result)
    }

    func installManager(managerId: String, withReply reply: @escaping (Int64) -> Void) {
        let taskId = managerId.withCString { helm_install_manager($0) }
        logger.info("helm_install_manager(\(managerId)) result: \(taskId)")
        reply(taskId)
    }

    func uninstallManager(managerId: String, withReply reply: @escaping (Int64) -> Void) {
        let taskId = managerId.withCString { helm_uninstall_manager($0) }
        logger.info("helm_uninstall_manager(\(managerId)) result: \(taskId)")
        reply(taskId)
    }
}
