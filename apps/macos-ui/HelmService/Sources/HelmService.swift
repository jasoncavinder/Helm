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

    func getSafeMode(withReply reply: @escaping (Bool) -> Void) {
        let enabled = helm_get_safe_mode()
        reply(enabled)
    }

    func setSafeMode(enabled: Bool, withReply reply: @escaping (Bool) -> Void) {
        let result = helm_set_safe_mode(enabled)
        logger.info("helm_set_safe_mode(\(enabled)) result: \(result)")
        reply(result)
    }

    func getHomebrewKegAutoCleanup(withReply reply: @escaping (Bool) -> Void) {
        let enabled = helm_get_homebrew_keg_auto_cleanup()
        reply(enabled)
    }

    func setHomebrewKegAutoCleanup(enabled: Bool, withReply reply: @escaping (Bool) -> Void) {
        let result = helm_set_homebrew_keg_auto_cleanup(enabled)
        logger.info("helm_set_homebrew_keg_auto_cleanup(\(enabled)) result: \(result)")
        reply(result)
    }

    func listPackageKegPolicies(withReply reply: @escaping (String?) -> Void) {
        guard let cString = helm_list_package_keg_policies() else {
            logger.warning("helm_list_package_keg_policies returned nil")
            reply(nil)
            return
        }
        defer { helm_free_string(cString) }
        reply(String(cString: cString))
    }

    func setPackageKegPolicy(managerId: String, packageName: String, policyMode: Int32, withReply reply: @escaping (Bool) -> Void) {
        let result = managerId.withCString { manager in
            packageName.withCString { package in
                helm_set_package_keg_policy(manager, package, policyMode)
            }
        }
        logger.info("helm_set_package_keg_policy(\(managerId), \(packageName), \(policyMode)) result: \(result)")
        reply(result)
    }

    func upgradeAll(includePinned: Bool, allowOsUpdates: Bool, withReply reply: @escaping (Bool) -> Void) {
        let result = helm_upgrade_all(includePinned, allowOsUpdates)
        logger.info("helm_upgrade_all(includePinned: \(includePinned), allowOsUpdates: \(allowOsUpdates)) result: \(result)")
        reply(result)
    }

    func upgradePackage(managerId: String, packageName: String, withReply reply: @escaping (Int64) -> Void) {
        let taskId = managerId.withCString { manager in
            packageName.withCString { package in
                helm_upgrade_package(manager, package)
            }
        }
        logger.info("helm_upgrade_package(\(managerId), \(packageName)) result: \(taskId)")
        reply(taskId)
    }

    func listPins(withReply reply: @escaping (String?) -> Void) {
        guard let cString = helm_list_pins() else {
            logger.warning("helm_list_pins returned nil")
            reply(nil)
            return
        }
        defer { helm_free_string(cString) }
        reply(String(cString: cString))
    }

    func pinPackage(managerId: String, packageName: String, version: String?, withReply reply: @escaping (Bool) -> Void) {
        let result: Bool
        if let version {
            result = managerId.withCString { manager in
                packageName.withCString { package in
                    version.withCString { versionPtr in
                        helm_pin_package(manager, package, versionPtr)
                    }
                }
            }
        } else {
            result = managerId.withCString { manager in
                packageName.withCString { package in
                    helm_pin_package(manager, package, nil)
                }
            }
        }
        logger.info("helm_pin_package(\(managerId), \(packageName)) result: \(result)")
        reply(result)
    }

    func unpinPackage(managerId: String, packageName: String, withReply reply: @escaping (Bool) -> Void) {
        let result = managerId.withCString { manager in
            packageName.withCString { package in
                helm_unpin_package(manager, package)
            }
        }
        logger.info("helm_unpin_package(\(managerId), \(packageName)) result: \(result)")
        reply(result)
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

    func updateManager(managerId: String, withReply reply: @escaping (Int64) -> Void) {
        let taskId = managerId.withCString { helm_update_manager($0) }
        logger.info("helm_update_manager(\(managerId)) result: \(taskId)")
        reply(taskId)
    }

    func uninstallManager(managerId: String, withReply reply: @escaping (Int64) -> Void) {
        let taskId = managerId.withCString { helm_uninstall_manager($0) }
        logger.info("helm_uninstall_manager(\(managerId)) result: \(taskId)")
        reply(taskId)
    }

    func resetDatabase(withReply reply: @escaping (Bool) -> Void) {
        let result = helm_reset_database()
        logger.info("helm_reset_database result: \(result)")
        reply(result)
    }
}
