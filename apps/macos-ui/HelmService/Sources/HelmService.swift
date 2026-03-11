import Foundation
import os.log

private let logger = Logger(subsystem: "app.jasoncavinder.Helm.HelmService", category: "service")

private func withOptionalCString<T>(_ value: String?, _ body: (UnsafePointer<CChar>?) -> T) -> T {
    guard let value else {
        return body(nil)
    }
    return value.withCString { body($0) }
}

class HelmService: NSObject, HelmServiceProtocol {
    private struct HelmCliShimInstallResponse: Codable {
        let accepted: Bool
        let installed: Bool
        let channel: String
        let updatePolicy: String
        let currentVersion: String?
        let shimPath: String?
        let markerPath: String?
        let reason: String?

        enum CodingKeys: String, CodingKey {
            case accepted
            case installed
            case channel
            case updatePolicy = "update_policy"
            case currentVersion = "current_version"
            case shimPath = "shim_path"
            case markerPath = "marker_path"
            case reason
        }
    }

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

    func getRustupToolchainDetail(toolchain: String, withReply reply: @escaping (String?) -> Void) {
        guard let cString = toolchain.withCString({ helm_get_rustup_toolchain_detail($0) }) else {
            logger.warning("helm_get_rustup_toolchain_detail(\(toolchain, privacy: .public)) returned nil")
            reply(nil)
            return
        }
        defer { helm_free_string(cString) }
        reply(String(cString: cString))
    }

    func addRustupComponent(toolchain: String, component: String, withReply reply: @escaping (Int64) -> Void) {
        let taskId = toolchain.withCString { toolchainPtr in
            component.withCString { componentPtr in
                helm_rustup_add_component(toolchainPtr, componentPtr)
            }
        }
        logger.info("helm_rustup_add_component(\(toolchain, privacy: .public), \(component, privacy: .public)) result: \(taskId)")
        reply(taskId)
    }

    func removeRustupComponent(toolchain: String, component: String, withReply reply: @escaping (Int64) -> Void) {
        let taskId = toolchain.withCString { toolchainPtr in
            component.withCString { componentPtr in
                helm_rustup_remove_component(toolchainPtr, componentPtr)
            }
        }
        logger.info("helm_rustup_remove_component(\(toolchain, privacy: .public), \(component, privacy: .public)) result: \(taskId)")
        reply(taskId)
    }

    func addRustupTarget(toolchain: String, target: String, withReply reply: @escaping (Int64) -> Void) {
        let taskId = toolchain.withCString { toolchainPtr in
            target.withCString { targetPtr in
                helm_rustup_add_target(toolchainPtr, targetPtr)
            }
        }
        logger.info("helm_rustup_add_target(\(toolchain, privacy: .public), \(target, privacy: .public)) result: \(taskId)")
        reply(taskId)
    }

    func removeRustupTarget(toolchain: String, target: String, withReply reply: @escaping (Int64) -> Void) {
        let taskId = toolchain.withCString { toolchainPtr in
            target.withCString { targetPtr in
                helm_rustup_remove_target(toolchainPtr, targetPtr)
            }
        }
        logger.info("helm_rustup_remove_target(\(toolchain, privacy: .public), \(target, privacy: .public)) result: \(taskId)")
        reply(taskId)
    }

    func setRustupDefaultToolchain(toolchain: String, withReply reply: @escaping (Int64) -> Void) {
        let taskId = toolchain.withCString { helm_rustup_set_default_toolchain($0) }
        logger.info("helm_rustup_set_default_toolchain(\(toolchain, privacy: .public)) result: \(taskId)")
        reply(taskId)
    }

    func setRustupOverride(toolchain: String, path: String, withReply reply: @escaping (Int64) -> Void) {
        let taskId = toolchain.withCString { toolchainPtr in
            path.withCString { pathPtr in
                helm_rustup_set_override(toolchainPtr, pathPtr)
            }
        }
        logger.info("helm_rustup_set_override(\(toolchain, privacy: .public), \(path, privacy: .public)) result: \(taskId)")
        reply(taskId)
    }

    func unsetRustupOverride(toolchain: String, path: String, withReply reply: @escaping (Int64) -> Void) {
        let taskId = toolchain.withCString { toolchainPtr in
            path.withCString { pathPtr in
                helm_rustup_unset_override(toolchainPtr, pathPtr)
            }
        }
        logger.info("helm_rustup_unset_override(\(toolchain, privacy: .public), \(path, privacy: .public)) result: \(taskId)")
        reply(taskId)
    }

    func setRustupProfile(profile: String, withReply reply: @escaping (Int64) -> Void) {
        let taskId = profile.withCString { helm_rustup_set_profile($0) }
        logger.info("helm_rustup_set_profile(\(profile, privacy: .public)) result: \(taskId)")
        reply(taskId)
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

    func getTaskOutput(taskId: Int64, withReply reply: @escaping (String?) -> Void) {
        guard let cString = helm_get_task_output(taskId) else {
            logger.warning("helm_get_task_output(\(taskId)) returned nil")
            reply(nil)
            return
        }
        defer { helm_free_string(cString) }
        reply(String(cString: cString))
    }

    func listTaskLogs(taskId: Int64, limit: Int64, withReply reply: @escaping (String?) -> Void) {
        guard let cString = helm_list_task_logs(taskId, limit) else {
            logger.warning("helm_list_task_logs(\(taskId), \(limit)) returned nil")
            reply(nil)
            return
        }
        defer { helm_free_string(cString) }
        reply(String(cString: cString))
    }

    func listTaskTimeoutPrompts(withReply reply: @escaping (String?) -> Void) {
        guard let cString = helm_list_task_timeout_prompts() else {
            logger.warning("helm_list_task_timeout_prompts returned nil")
            reply(nil)
            return
        }
        defer { helm_free_string(cString) }
        reply(String(cString: cString))
    }

    func respondTaskTimeoutPrompt(taskId: Int64, waitForCompletion: Bool, withReply reply: @escaping (Bool) -> Void) {
        let result = helm_respond_task_timeout_prompt(taskId, waitForCompletion)
        logger.info(
            "helm_respond_task_timeout_prompt(\(taskId), wait=\(waitForCompletion)) result: \(result)"
        )
        reply(result)
    }

    func triggerRefresh(withReply reply: @escaping (Bool) -> Void) {
        let result = helm_trigger_refresh()
        logger.info("helm_trigger_refresh result: \(result)")
        reply(result)
    }

    func triggerDetection(withReply reply: @escaping (Bool) -> Void) {
        let result = helm_trigger_detection()
        logger.info("helm_trigger_detection result: \(result)")
        reply(result)
    }

    func triggerDetectionForManager(managerId: String, withReply reply: @escaping (Bool) -> Void) {
        let result = managerId.withCString { manager in
            helm_trigger_detection_for_manager(manager)
        }
        logger.info("helm_trigger_detection_for_manager(\(managerId)) result: \(result)")
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

    func triggerRemoteSearchForManager(managerId: String, query: String, withReply reply: @escaping (Int64) -> Void) {
        let taskId = managerId.withCString { manager in
            query.withCString { searchQuery in
                helm_trigger_remote_search_for_manager(manager, searchQuery)
            }
        }
        logger.info("helm_trigger_remote_search_for_manager(\(managerId)) result: \(taskId)")
        reply(taskId)
    }

    func cancelTask(taskId: Int64, withReply reply: @escaping (Bool) -> Void) {
        let result = helm_cancel_task(taskId)
        logger.info("helm_cancel_task(\(taskId)) result: \(result)")
        reply(result)
    }

    func dismissTask(taskId: Int64, withReply reply: @escaping (Bool) -> Void) {
        let result = helm_dismiss_task(taskId)
        logger.info("helm_dismiss_task(\(taskId)) result: \(result)")
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

    func doctorScan(withReply reply: @escaping (String?) -> Void) {
        guard let cString = helm_doctor_scan() else {
            logger.warning("helm_doctor_scan returned nil")
            reply(nil)
            return
        }
        defer { helm_free_string(cString) }
        reply(String(cString: cString))
    }

    func getSharedOnboardingState(withReply reply: @escaping (Bool, String?) -> Void) {
        let completed = helm_get_cli_onboarding_completed()
        guard let cString = helm_get_cli_accepted_license_terms_version() else {
            reply(completed, nil)
            return
        }
        defer { helm_free_string(cString) }
        reply(completed, String(cString: cString))
    }

    func setSharedOnboardingCompleted(completed: Bool, withReply reply: @escaping (Bool) -> Void) {
        let result = helm_set_cli_onboarding_completed(completed)
        logger.info("helm_set_cli_onboarding_completed(\(completed)) result: \(result)")
        reply(result)
    }

    func setSharedAcceptedLicenseTermsVersion(version: String?, withReply reply: @escaping (Bool) -> Void) {
        let result: Bool
        if let version {
            result = version.withCString { versionPtr in
                helm_set_cli_accepted_license_terms_version(versionPtr)
            }
        } else {
            result = helm_set_cli_accepted_license_terms_version(nil)
        }
        logger.info("helm_set_cli_accepted_license_terms_version(\(version ?? "nil")) result: \(result)")
        reply(result)
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

    func installHelmCliShim(
        appBundlePath: String,
        appBundleIdentifier: String,
        withReply reply: @escaping (String?) -> Void
    ) {
        let appBundleURL = URL(fileURLWithPath: appBundlePath, isDirectory: true)
        let cliURL = appBundleURL
            .appendingPathComponent("Contents", isDirectory: true)
            .appendingPathComponent("Resources", isDirectory: true)
            .appendingPathComponent("helm-cli", isDirectory: false)

        guard FileManager.default.isExecutableFile(atPath: cliURL.path) else {
            logger.warning("installHelmCliShim missing bundled CLI at \(cliURL.path, privacy: .public)")
            reply(encodeHelmCliShimInstallResponse(
                HelmCliShimInstallResponse(
                    accepted: false,
                    installed: false,
                    channel: "app-bundle-shim",
                    updatePolicy: "channel",
                    currentVersion: nil,
                    shimPath: nil,
                    markerPath: nil,
                    reason: "Bundled Helm CLI binary is missing from the app bundle."
                )
            ))
            return
        }

        let process = Process()
        process.executableURL = cliURL
        process.arguments = [
            "--json",
            "self",
            "install-shim",
            "--app-bundle-path",
            appBundlePath,
            "--app-bundle-id",
            appBundleIdentifier,
        ]
        let stdoutPipe = Pipe()
        let stderrPipe = Pipe()
        process.standardOutput = stdoutPipe
        process.standardError = stderrPipe

        do {
            try process.run()
            process.waitUntilExit()
        } catch {
            logger.error("installHelmCliShim failed to launch bundled CLI: \(error.localizedDescription, privacy: .public)")
            reply(encodeHelmCliShimInstallResponse(
                HelmCliShimInstallResponse(
                    accepted: false,
                    installed: false,
                    channel: "app-bundle-shim",
                    updatePolicy: "channel",
                    currentVersion: nil,
                    shimPath: nil,
                    markerPath: nil,
                    reason: error.localizedDescription
                )
            ))
            return
        }

        let stdout = String(
            data: stdoutPipe.fileHandleForReading.readDataToEndOfFile(),
            encoding: .utf8
        )?.trimmingCharacters(in: .whitespacesAndNewlines)
        let stderr = String(
            data: stderrPipe.fileHandleForReading.readDataToEndOfFile(),
            encoding: .utf8
        )?.trimmingCharacters(in: .whitespacesAndNewlines)

        if let stdout, !stdout.isEmpty {
            logger.info("installHelmCliShim bundled CLI exited \(process.terminationStatus) with JSON payload")
            reply(stdout)
            return
        }

        let reason: String
        if let stderr, !stderr.isEmpty {
            reason = stderr
        } else {
            reason = "Bundled Helm CLI shim install failed without output."
        }
        logger.error("installHelmCliShim bundled CLI exited \(process.terminationStatus) without JSON payload: \(reason, privacy: .public)")
        reply(encodeHelmCliShimInstallResponse(
            HelmCliShimInstallResponse(
                accepted: false,
                installed: false,
                channel: "app-bundle-shim",
                updatePolicy: "channel",
                currentVersion: nil,
                shimPath: nil,
                markerPath: nil,
                reason: reason
            )
        ))
    }

    func listPackageManagerPreferences(withReply reply: @escaping (String?) -> Void) {
        guard let cString = helm_list_package_manager_preferences() else {
            logger.warning("helm_list_package_manager_preferences returned nil")
            reply(nil)
            return
        }
        defer { helm_free_string(cString) }
        reply(String(cString: cString))
    }

    func setPackageManagerPreference(packageFamilyKey: String, managerId: String?, withReply reply: @escaping (Bool) -> Void) {
        let result = packageFamilyKey.withCString { package in
            if let managerId {
                return managerId.withCString { manager in
                    helm_set_package_manager_preference(package, manager)
                }
            }
            return helm_set_package_manager_preference(package, nil)
        }
        logger.info(
            "helm_set_package_manager_preference(\(packageFamilyKey), \(managerId ?? "nil")) result: \(result)"
        )
        reply(result)
    }

    func previewUpgradePlan(includePinned: Bool, allowOsUpdates: Bool, withReply reply: @escaping (String?) -> Void) {
        guard let cString = helm_preview_upgrade_plan(includePinned, allowOsUpdates) else {
            logger.warning("helm_preview_upgrade_plan returned nil")
            reply(nil)
            return
        }
        defer { helm_free_string(cString) }
        reply(String(cString: cString))
    }

    func upgradeAll(includePinned: Bool, allowOsUpdates: Bool, withReply reply: @escaping (Bool) -> Void) {
        let result = helm_upgrade_all(includePinned, allowOsUpdates)
        logger.info("helm_upgrade_all(includePinned: \(includePinned), allowOsUpdates: \(allowOsUpdates)) result: \(result)")
        reply(result)
    }

    func upgradePackage(
        managerId: String,
        packageName: String,
        packageTargetName: String?,
        version: String?,
        withReply reply: @escaping (Int64) -> Void
    ) {
        let taskId = managerId.withCString { manager in
            packageName.withCString { package in
                withOptionalCString(packageTargetName) { targetPtr in
                    withOptionalCString(version) { versionPtr in
                        helm_upgrade_package(manager, package, targetPtr, versionPtr)
                    }
                }
            }
        }
        logger.info(
            "helm_upgrade_package(\(managerId), \(packageName), target=\(packageTargetName ?? "-", privacy: .public), version=\(version ?? "-", privacy: .public)) result: \(taskId)"
        )
        reply(taskId)
    }

    func installPackage(
        managerId: String,
        packageName: String,
        packageTargetName: String?,
        version: String?,
        withReply reply: @escaping (Int64) -> Void
    ) {
        let taskId = managerId.withCString { manager in
            packageName.withCString { package in
                withOptionalCString(packageTargetName) { targetPtr in
                    withOptionalCString(version) { versionPtr in
                        helm_install_package(manager, package, targetPtr, versionPtr)
                    }
                }
            }
        }
        logger.info(
            "helm_install_package(\(managerId), \(packageName), target=\(packageTargetName ?? "-", privacy: .public), version=\(version ?? "-", privacy: .public)) result: \(taskId)"
        )
        reply(taskId)
    }

    func uninstallPackage(
        managerId: String,
        packageName: String,
        packageTargetName: String?,
        version: String?,
        withReply reply: @escaping (Int64) -> Void
    ) {
        let taskId = managerId.withCString { manager in
            packageName.withCString { package in
                withOptionalCString(packageTargetName) { targetPtr in
                    withOptionalCString(version) { versionPtr in
                        helm_uninstall_package(manager, package, targetPtr, versionPtr)
                    }
                }
            }
        }
        logger.info(
            "helm_uninstall_package(\(managerId), \(packageName), target=\(packageTargetName ?? "-", privacy: .public), version=\(version ?? "-", privacy: .public)) result: \(taskId)"
        )
        reply(taskId)
    }

    func previewPackageUninstall(managerId: String, packageName: String, version: String?, withReply reply: @escaping (String?) -> Void) {
        guard let cString = managerId.withCString({ manager in
            packageName.withCString { package in
                withOptionalCString(version) { versionPtr in
                    helm_preview_package_uninstall(manager, package, versionPtr)
                }
            }
        }) else {
            logger.warning("helm_preview_package_uninstall(\(managerId), \(packageName), version=\(version ?? "-", privacy: .public)) returned nil")
            reply(nil)
            return
        }
        defer { helm_free_string(cString) }
        reply(String(cString: cString))
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

    func unpinPackage(managerId: String, packageName: String, version: String?, withReply reply: @escaping (Bool) -> Void) {
        let result: Bool
        if let version {
            result = managerId.withCString { manager in
                packageName.withCString { package in
                    version.withCString { versionPtr in
                        helm_unpin_package(manager, package, versionPtr)
                    }
                }
            }
        } else {
            result = managerId.withCString { manager in
                packageName.withCString { package in
                    helm_unpin_package(manager, package, nil)
                }
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

    func setManagerSelectedExecutablePath(managerId: String, selectedPath: String?, withReply reply: @escaping (Bool) -> Void) {
        let result: Bool
        if let selectedPath {
            result = managerId.withCString { manager in
                selectedPath.withCString { selected in
                    helm_set_manager_selected_executable_path(manager, selected)
                }
            }
        } else {
            result = managerId.withCString { manager in
                helm_set_manager_selected_executable_path(manager, nil)
            }
        }
        logger.info("helm_set_manager_selected_executable_path(\(managerId), \(selectedPath ?? "nil")) result: \(result)")
        reply(result)
    }

    func setManagerActiveInstallInstance(managerId: String, instanceId: String, withReply reply: @escaping (Bool) -> Void) {
        let result = managerId.withCString { manager in
            instanceId.withCString { instance in
                helm_set_manager_active_install_instance(manager, instance)
            }
        }
        logger.info("helm_set_manager_active_install_instance(\(managerId), \(instanceId)) result: \(result)")
        reply(result)
    }

    func acknowledgeManagerMultiInstanceState(managerId: String, withReply reply: @escaping (Bool) -> Void) {
        let result = managerId.withCString { manager in
            helm_ack_manager_multi_instance_state(manager)
        }
        logger.info("helm_ack_manager_multi_instance_state(\(managerId)) result: \(result)")
        reply(result)
    }

    func clearManagerMultiInstanceAck(managerId: String, withReply reply: @escaping (Bool) -> Void) {
        let result = managerId.withCString { manager in
            helm_clear_manager_multi_instance_ack(manager)
        }
        logger.info("helm_clear_manager_multi_instance_ack(\(managerId)) result: \(result)")
        reply(result)
    }

    func setManagerInstallMethod(managerId: String, installMethod: String?, withReply reply: @escaping (Bool) -> Void) {
        let result: Bool
        if let installMethod {
            result = managerId.withCString { manager in
                installMethod.withCString { method in
                    helm_set_manager_install_method(manager, method)
                }
            }
        } else {
            result = managerId.withCString { manager in
                helm_set_manager_install_method(manager, nil)
            }
        }
        logger.info("helm_set_manager_install_method(\(managerId), \(installMethod ?? "nil")) result: \(result)")
        reply(result)
    }

    func setManagerTimeoutProfile(
        managerId: String,
        hardTimeoutSeconds: Int64,
        idleTimeoutSeconds: Int64,
        withReply reply: @escaping (Bool) -> Void
    ) {
        let result = managerId.withCString { manager in
            helm_set_manager_timeout_profile(manager, hardTimeoutSeconds, idleTimeoutSeconds)
        }
        logger.info(
            "helm_set_manager_timeout_profile(\(managerId), hard=\(hardTimeoutSeconds), idle=\(idleTimeoutSeconds)) result: \(result)"
        )
        reply(result)
    }

    func previewManagerUninstall(
        managerId: String,
        allowUnknownProvenance: Bool,
        withReply reply: @escaping (String?) -> Void
    ) {
        let optionsJson = allowUnknownProvenance
            ? #"{"allowUnknownProvenance":true}"#
            : nil
        previewManagerUninstallWithOptions(
            managerId: managerId,
            optionsJson: optionsJson,
            withReply: reply
        )
    }

    func previewManagerUninstallWithOptions(
        managerId: String,
        optionsJson: String?,
        withReply reply: @escaping (String?) -> Void
    ) {
        let cString: UnsafeMutablePointer<CChar>?
        if let optionsJson {
            cString = managerId.withCString { manager in
                optionsJson.withCString { options in
                    helm_preview_manager_uninstall_with_options(manager, options)
                }
            }
        } else {
            cString = managerId.withCString { manager in
                helm_preview_manager_uninstall_with_options(manager, nil)
            }
        }
        guard let cString else {
            logger.warning(
                "helm_preview_manager_uninstall_with_options(\(managerId), options=\(optionsJson ?? "nil")) returned nil"
            )
            reply(nil)
            return
        }
        defer { helm_free_string(cString) }
        reply(String(cString: cString))
    }

    func installManager(managerId: String, withReply reply: @escaping (Int64) -> Void) {
        installManagerWithOptions(
            managerId: managerId,
            optionsJson: nil,
            withReply: reply
        )
    }

    func installManagerWithOptions(
        managerId: String,
        optionsJson: String?,
        withReply reply: @escaping (Int64) -> Void
    ) {
        let taskId: Int64
        if let optionsJson {
            taskId = managerId.withCString { manager in
                optionsJson.withCString { options in
                    helm_install_manager_with_options(manager, options)
                }
            }
        } else {
            taskId = managerId.withCString { manager in
                helm_install_manager_with_options(manager, nil)
            }
        }
        logger.info(
            "helm_install_manager_with_options(\(managerId), options=\(optionsJson ?? "nil")) result: \(taskId)"
        )
        reply(taskId)
    }

    func updateManager(managerId: String, withReply reply: @escaping (Int64) -> Void) {
        let taskId = managerId.withCString { helm_update_manager($0) }
        logger.info("helm_update_manager(\(managerId)) result: \(taskId)")
        reply(taskId)
    }

    func applyManagerPackageStateIssueRepair(
        managerId: String,
        sourceManagerId: String,
        packageName: String,
        issueCode: String,
        optionId: String,
        withReply reply: @escaping (Int64) -> Void
    ) {
        let taskId = managerId.withCString { manager in
            sourceManagerId.withCString { sourceManager in
                packageName.withCString { package in
                    issueCode.withCString { issue in
                        optionId.withCString { option in
                            helm_apply_manager_package_state_issue_repair(
                                manager,
                                sourceManager,
                                package,
                                issue,
                                option
                            )
                        }
                    }
                }
            }
        }
        logger.info(
            "helm_apply_manager_package_state_issue_repair(\(managerId), \(sourceManagerId), \(packageName), \(issueCode), \(optionId)) result: \(taskId)"
        )
        reply(taskId)
    }

    func uninstallManager(managerId: String, withReply reply: @escaping (Int64) -> Void) {
        uninstallManagerWithOptions(
            managerId: managerId,
            allowUnknownProvenance: false,
            withReply: reply
        )
    }

    func uninstallManagerWithOptions(
        managerId: String,
        allowUnknownProvenance: Bool,
        withReply reply: @escaping (Int64) -> Void
    ) {
        let optionsJson = allowUnknownProvenance
            ? #"{"allowUnknownProvenance":true}"#
            : nil
        uninstallManagerWithUninstallOptions(
            managerId: managerId,
            optionsJson: optionsJson,
            withReply: reply
        )
    }

    func uninstallManagerWithUninstallOptions(
        managerId: String,
        optionsJson: String?,
        withReply reply: @escaping (Int64) -> Void
    ) {
        let taskId: Int64
        if let optionsJson {
            taskId = managerId.withCString { manager in
                optionsJson.withCString { options in
                    helm_uninstall_manager_with_uninstall_options(manager, options)
                }
            }
        } else {
            taskId = managerId.withCString { manager in
                helm_uninstall_manager_with_uninstall_options(manager, nil)
            }
        }
        logger.info(
            "helm_uninstall_manager_with_uninstall_options(\(managerId), options=\(optionsJson ?? "nil")) result: \(taskId)"
        )
        reply(taskId)
    }

    func resetDatabase(withReply reply: @escaping (Bool) -> Void) {
        let result = helm_reset_database()
        logger.info("helm_reset_database result: \(result)")
        reply(result)
    }

    func takeLastErrorKey(withReply reply: @escaping (String?) -> Void) {
        guard let cString = helm_take_last_error_key() else {
            reply(nil)
            return
        }
        defer { helm_free_string(cString) }
        reply(String(cString: cString))
    }

    private func encodeHelmCliShimInstallResponse(_ response: HelmCliShimInstallResponse) -> String? {
        let encoder = JSONEncoder()
        guard let data = try? encoder.encode(response) else {
            return nil
        }
        return String(data: data, encoding: .utf8)
    }
}
