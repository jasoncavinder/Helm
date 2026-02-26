import AppKit
import Foundation
import os.log
import Darwin
import ServiceManagement

// swiftlint:disable file_length

private let logger = Logger(subsystem: "com.jasoncavinder.Helm", category: "core.settings")

extension HelmCore {
    static let allManagersScopeId = UpgradePreviewPlanner.allManagersScopeId
    private static let legacyLoginItemBundleIdentifier = "com.jasoncavinder.HelmLoginHelper"
    private static let legacyLoginItemAppName = "HelmLoginHelper.app"

    // MARK: - App Lifecycle

    var launchAtLoginSupported: Bool {
        if #available(macOS 13.0, *) {
            return true
        }
        return legacyLoginItemIsAvailable()
    }

    private func legacyLoginItemBundleURL() -> URL {
        Bundle.main.bundleURL
            .appendingPathComponent("Contents", isDirectory: true)
            .appendingPathComponent("Library", isDirectory: true)
            .appendingPathComponent("LoginItems", isDirectory: true)
            .appendingPathComponent(Self.legacyLoginItemAppName, isDirectory: true)
    }

    private func legacyLoginItemIsAvailable() -> Bool {
        FileManager.default.fileExists(atPath: legacyLoginItemBundleURL().path)
    }

    private func legacyLoginItemIsEnabled() -> Bool {
        if launchctlJobIsLoaded(
            arguments: [
                "print",
                "gui/\(getuid())/\(Self.legacyLoginItemBundleIdentifier)",
            ]
        ) {
            return true
        }

        if launchctlJobIsLoaded(arguments: ["list", Self.legacyLoginItemBundleIdentifier]) {
            return true
        }

        return UserDefaults.standard.bool(forKey: Self.launchAtLoginEnabledKey)
    }

    private func launchctlJobIsLoaded(arguments: [String]) -> Bool {
        let task = Process()
        task.executableURL = URL(fileURLWithPath: "/bin/launchctl")
        task.arguments = arguments
        task.standardOutput = Pipe()
        task.standardError = Pipe()

        do {
            try task.run()
            task.waitUntilExit()
            return task.terminationStatus == 0
        } catch {
            logger.debug("launchctl status query failed: \(error.localizedDescription)")
            return false
        }
    }

    func refreshLaunchAtLogin() {
        guard launchAtLoginSupported else {
            DispatchQueue.main.async {
                UserDefaults.standard.removeObject(forKey: Self.launchAtLoginEnabledKey)
                self.launchAtLoginEnabled = false
            }
            return
        }

        if #available(macOS 13.0, *) {
            let enabled = SMAppService.mainApp.status == .enabled
            DispatchQueue.main.async {
                UserDefaults.standard.set(enabled, forKey: Self.launchAtLoginEnabledKey)
                self.launchAtLoginEnabled = enabled
            }
            return
        }

        let enabled = legacyLoginItemIsEnabled()
        DispatchQueue.main.async {
            UserDefaults.standard.set(enabled, forKey: Self.launchAtLoginEnabledKey)
            self.launchAtLoginEnabled = enabled
        }
    }

    func setLaunchAtLogin(_ enabled: Bool) {
        guard launchAtLoginSupported else {
            recordLastError(
                source: "core.settings",
                action: "setLaunchAtLogin.unsupported",
                taskType: "settings"
            )
            return
        }

        if #available(macOS 13.0, *) {
            do {
                if enabled {
                    try SMAppService.mainApp.register()
                } else {
                    try SMAppService.mainApp.unregister()
                }
            } catch {
                logger.error("setLaunchAtLogin(\(enabled)) failed: \(error.localizedDescription)")
                recordLastError(
                    source: "core.settings",
                    action: "setLaunchAtLogin",
                    taskType: "settings"
                )
            }
            refreshLaunchAtLogin()
            return
        }

        let success = SMLoginItemSetEnabled(Self.legacyLoginItemBundleIdentifier as CFString, enabled)
        if !success {
            logger.error("setLaunchAtLogin(\(enabled)) failed for legacy login helper")
            recordLastError(
                source: "core.settings",
                action: "setLaunchAtLogin.legacy",
                taskType: "settings"
            )
        }

        refreshLaunchAtLogin()
    }

    // MARK: - Bundled CLI Shim

    static func defaultHelmCliShimURL() -> URL {
        userHomeDirectoryURL()
            .appendingPathComponent(".local", isDirectory: true)
            .appendingPathComponent("bin", isDirectory: true)
            .appendingPathComponent("helm", isDirectory: false)
    }

    static func defaultHelmCliInstallMarkerURL() -> URL {
        userHomeDirectoryURL()
            .appendingPathComponent(".config", isDirectory: true)
            .appendingPathComponent("helm", isDirectory: true)
            .appendingPathComponent("install.json", isDirectory: false)
    }

    private static func userHomeDirectoryURL() -> URL {
        if let posixHomePath = posixHomeDirectoryPath() {
            return URL(fileURLWithPath: posixHomePath, isDirectory: true)
        }
        return FileManager.default.homeDirectoryForCurrentUser
    }

    private static func posixHomeDirectoryPath() -> String? {
        let uid = getuid()
        let initialBufferSize = max(1024, Int(sysconf(_SC_GETPW_R_SIZE_MAX)))
        var buffer = [CChar](repeating: 0, count: initialBufferSize)
        var pwd = passwd()
        var result: UnsafeMutablePointer<passwd>?

        let status = getpwuid_r(
            uid,
            &pwd,
            &buffer,
            buffer.count,
            &result
        )
        guard status == 0,
              let directoryPtr = result?.pointee.pw_dir else {
            return nil
        }
        let path = String(cString: directoryPtr)
        return path.isEmpty ? nil : path
    }

    func refreshHelmCliShimStatus() {
        let status = HelmCliShimInstaller.status(
            bundle: .main,
            shimURL: Self.defaultHelmCliShimURL()
        )

        DispatchQueue.main.async {
            self.helmCliBundledAvailable = status.bundledCliPath != nil
            self.helmCliBundledPath = status.bundledCliPath
            self.helmCliShimInstalled = status.shimInstalled
            self.helmCliShimPath = Self.defaultHelmCliShimURL().path
        }
    }

    func installHelmCliShim() {
        guard !helmCliShimOperationInProgress else { return }

        DispatchQueue.main.async {
            self.helmCliShimOperationInProgress = true
            self.helmCliShimStatusMessage = nil
        }

        do {
            let installResult = try HelmCliShimInstaller.install(
                bundle: .main,
                shimURL: Self.defaultHelmCliShimURL(),
                markerURL: Self.defaultHelmCliInstallMarkerURL()
            )

            logger.info("Installed Helm CLI shim at \(installResult.shimPath, privacy: .public)")
            DispatchQueue.main.async {
                self.helmCliShimStatusMessage = L10n.App.Settings.CLI.Message.installSuccess.localized
            }
        } catch let error as HelmCliShimInstaller.Error {
            logger.error("installHelmCliShim failed: \(error.logDescription, privacy: .public)")
            recordLastError(
                source: "core.settings",
                action: "installHelmCliShim",
                taskType: "settings"
            )
            DispatchQueue.main.async {
                self.helmCliShimStatusMessage = self.messageForHelmCliShimError(
                    error,
                    installOperation: true
                )
            }
        } catch {
            logger.error("installHelmCliShim failed: \(error.localizedDescription, privacy: .public)")
            recordLastError(
                source: "core.settings",
                action: "installHelmCliShim",
                taskType: "settings"
            )
            DispatchQueue.main.async {
                self.helmCliShimStatusMessage = L10n.App.Settings.CLI.Message.installFailed.localized
            }
        }

        refreshHelmCliShimStatus()
        DispatchQueue.main.async {
            self.helmCliShimOperationInProgress = false
        }
    }

    func removeHelmCliShim() {
        guard !helmCliShimOperationInProgress else { return }

        DispatchQueue.main.async {
            self.helmCliShimOperationInProgress = true
            self.helmCliShimStatusMessage = nil
        }

        do {
            let removed = try HelmCliShimInstaller.remove(
                shimURL: Self.defaultHelmCliShimURL(),
                markerURL: Self.defaultHelmCliInstallMarkerURL()
            )
            if removed {
                logger.info("Removed Helm CLI shim at \(Self.defaultHelmCliShimURL().path, privacy: .public)")
            } else {
                logger.info("Helm CLI shim remove requested, but shim was already absent.")
            }
            DispatchQueue.main.async {
                self.helmCliShimStatusMessage = L10n.App.Settings.CLI.Message.removeSuccess.localized
            }
        } catch let error as HelmCliShimInstaller.Error {
            logger.error("removeHelmCliShim failed: \(error.logDescription, privacy: .public)")
            recordLastError(
                source: "core.settings",
                action: "removeHelmCliShim",
                taskType: "settings"
            )
            DispatchQueue.main.async {
                self.helmCliShimStatusMessage = self.messageForHelmCliShimError(
                    error,
                    installOperation: false
                )
            }
        } catch {
            logger.error("removeHelmCliShim failed: \(error.localizedDescription, privacy: .public)")
            recordLastError(
                source: "core.settings",
                action: "removeHelmCliShim",
                taskType: "settings"
            )
            DispatchQueue.main.async {
                self.helmCliShimStatusMessage = L10n.App.Settings.CLI.Message.removeFailed.localized
            }
        }

        refreshHelmCliShimStatus()
        DispatchQueue.main.async {
            self.helmCliShimOperationInProgress = false
        }
    }

    private func messageForHelmCliShimError(
        _ error: HelmCliShimInstaller.Error,
        installOperation: Bool
    ) -> String {
        switch error {
        case .bundledCliMissing:
            return L10n.App.Settings.CLI.Message.bundleUnavailable.localized
        case .existingNonManagedShim:
            return L10n.App.Settings.CLI.Message.existingInstallConflict.localized
        case .removeRefusedNonManagedShim:
            return L10n.App.Settings.CLI.Message.removeBlockedNotManaged.localized
        case .ioFailure:
            return installOperation
                ? L10n.App.Settings.CLI.Message.installFailed.localized
                : L10n.App.Settings.CLI.Message.removeFailed.localized
        }
    }

    // MARK: - Safe Mode

    func fetchSafeMode() {
        service()?.getSafeMode { [weak self] enabled in
            DispatchQueue.main.async {
                self?.safeModeEnabled = enabled
            }
        }
    }

    func setSafeMode(_ enabled: Bool) {
        guard let service = service() else {
            recordLastError(
                source: "core.settings",
                action: "setSafeMode.service_unavailable",
                taskType: "settings"
            )
            return
        }
        service.setSafeMode(enabled: enabled) { [weak self] success in
            DispatchQueue.main.async {
                if success {
                    self?.safeModeEnabled = enabled
                } else {
                    logger.error("setSafeMode(\(enabled)) failed")
                    self?.recordLastError(
                        source: "core.settings",
                        action: "setSafeMode",
                        taskType: "settings"
                    )
                }
            }
        }
    }

    // MARK: - Keg Cleanup

    func fetchHomebrewKegAutoCleanup() {
        service()?.getHomebrewKegAutoCleanup { [weak self] enabled in
            DispatchQueue.main.async {
                self?.homebrewKegAutoCleanupEnabled = enabled
            }
        }
    }

    func setHomebrewKegAutoCleanup(_ enabled: Bool) {
        guard let service = service() else {
            recordLastError(
                source: "core.settings",
                action: "setHomebrewKegAutoCleanup.service_unavailable",
                managerId: "homebrew_formula",
                taskType: "settings"
            )
            return
        }
        service.setHomebrewKegAutoCleanup(enabled: enabled) { [weak self] success in
            DispatchQueue.main.async {
                if success {
                    self?.homebrewKegAutoCleanupEnabled = enabled
                } else {
                    logger.error("setHomebrewKegAutoCleanup(\(enabled)) failed")
                    self?.recordLastError(
                        source: "core.settings",
                        action: "setHomebrewKegAutoCleanup",
                        managerId: "homebrew_formula",
                        taskType: "settings"
                    )
                }
            }
        }
    }

    func decodeCorePayload<T: Decodable>(
        _ type: T.Type,
        from data: Data,
        decodeContext: String,
        source: String,
        action: String,
        managerId: String? = nil,
        taskType: String,
        keyDecodingStrategy: JSONDecoder.KeyDecodingStrategy = .convertFromSnakeCase
    ) -> T? {
        do {
            let decoder = JSONDecoder()
            decoder.keyDecodingStrategy = keyDecodingStrategy
            return try decoder.decode(type, from: data)
        } catch {
            logger.error("\(decodeContext): decode failed (\(data.count) bytes): \(error)")
            recordLastError(
                source: source,
                action: action,
                managerId: managerId,
                taskType: taskType
            )
            return nil
        }
    }

    private func decodeSettingsPayload<T: Decodable>(
        _ type: T.Type,
        from data: Data,
        decodeContext: String,
        action: String,
        managerId: String? = nil,
        taskType: String
    ) -> T? {
        decodeCorePayload(
            type,
            from: data,
            decodeContext: decodeContext,
            source: "core.settings",
            action: action,
            managerId: managerId,
            taskType: taskType
        )
    }

    // MARK: - Keg Policies

    func fetchPackageKegPolicies() {
        service()?.listPackageKegPolicies { [weak self] jsonString in
            guard let self = self,
                  let jsonString = jsonString,
                  let data = jsonString.data(using: .utf8),
                  let entries: [CorePackageKegPolicy] = self.decodeSettingsPayload(
                    [CorePackageKegPolicy].self,
                    from: data,
                    decodeContext: "fetchPackageKegPolicies",
                    action: "listPackageKegPolicies.decode",
                    managerId: "homebrew_formula",
                    taskType: "settings"
                  ) else { return }

            DispatchQueue.main.async {
                var overrides: [String: HomebrewKegPolicyOverride] = [:]
                for entry in entries where entry.managerId == "homebrew_formula" {
                    overrides["\(entry.managerId):\(entry.packageName)"] = entry.policy
                }
                self.packageKegPolicyOverrides = overrides
            }
        }
    }

    func kegPolicySelection(for package: PackageItem) -> KegPolicySelection {
        guard package.managerId == "homebrew_formula" else { return .useGlobal }

        switch packageKegPolicyOverrides[package.id] {
        case .keep:
            return .keep
        case .cleanup:
            return .cleanup
        case .none:
            return .useGlobal
        }
    }

    func setKegPolicySelection(for package: PackageItem, selection: KegPolicySelection) {
        guard package.managerId == "homebrew_formula" else { return }

        let policyMode: Int32
        switch selection {
        case .useGlobal:
            policyMode = -1
        case .keep:
            policyMode = 0
        case .cleanup:
            policyMode = 1
        }

        guard let service = service() else {
            recordLastError(
                source: "core.settings",
                action: "setPackageKegPolicy.service_unavailable",
                managerId: package.managerId,
                taskType: "settings"
            )
            return
        }

        service.setPackageKegPolicy(managerId: package.managerId, packageName: package.name, policyMode: policyMode) { [weak self] success in
            DispatchQueue.main.async {
                guard let self = self else { return }
                guard success else {
                    logger.error("setPackageKegPolicy(\(package.managerId):\(package.name), \(policyMode)) failed")
                    self.recordLastError(
                        source: "core.settings",
                        action: "setPackageKegPolicy",
                        managerId: package.managerId,
                        taskType: "settings"
                    )
                    return
                }
                switch selection {
                case .useGlobal:
                    self.packageKegPolicyOverrides.removeValue(forKey: package.id)
                case .keep:
                    self.packageKegPolicyOverrides[package.id] = .keep
                case .cleanup:
                    self.packageKegPolicyOverrides[package.id] = .cleanup
                }
            }
        }
    }

    // MARK: - Upgrade All

    func upgradeAll(includePinned: Bool = false, allowOsUpdates: Bool = false) {
        DispatchQueue.main.async {
            self.upgradePlanIncludePinned = includePinned
            self.upgradePlanAllowOsUpdates = allowOsUpdates
            for step in self.upgradePlanSteps {
                self.upgradePlanTaskProjectionByStepId.removeValue(forKey: step.id)
            }
            self.rebuildUpgradePlanFailureGroups()
        }
        guard let service = service() else {
            recordLastError(
                source: "core.settings",
                action: "upgradeAll.service_unavailable",
                taskType: "upgrade"
            )
            return
        }
        service.upgradeAll(includePinned: includePinned, allowOsUpdates: allowOsUpdates) { success in
            if !success {
                logger.error("upgradeAll(includePinned: \(includePinned), allowOsUpdates: \(allowOsUpdates)) failed")
                self.recordLastError(
                    source: "core.settings",
                    action: "upgradeAll",
                    taskType: "upgrade"
                )
            }
        }
    }

    func refreshUpgradePlan(includePinned: Bool = false, allowOsUpdates: Bool = false) {
        guard let service = service() else {
            recordLastError(
                source: "core.settings",
                action: "previewUpgradePlan.service_unavailable",
                taskType: "upgrade"
            )
            return
        }
        service.previewUpgradePlan(includePinned: includePinned, allowOsUpdates: allowOsUpdates) { [weak self] jsonString in
            guard let self = self else { return }
            guard let jsonString,
                  let data = jsonString.data(using: .utf8),
                  let steps: [CoreUpgradePlanStep] = self.decodeSettingsPayload(
                    [CoreUpgradePlanStep].self,
                    from: data,
                    decodeContext: "refreshUpgradePlan",
                    action: "previewUpgradePlan.decode",
                    taskType: "upgrade"
                  ) else { return }

            DispatchQueue.main.async {
                self.upgradePlanIncludePinned = includePinned
                self.upgradePlanAllowOsUpdates = allowOsUpdates
                self.upgradePlanSteps = steps.sorted { lhs, rhs in
                    lhs.orderIndex < rhs.orderIndex
                }
                self.syncUpgradePlanProjection(from: self.latestCoreTasksSnapshot)
            }
        }
    }

    func projectedUpgradePlanStatus(for step: CoreUpgradePlanStep) -> String {
        upgradePlanTaskProjectionByStepId[step.id]?.status ?? step.status
    }

    func projectedUpgradePlanTaskId(for step: CoreUpgradePlanStep) -> UInt64? {
        upgradePlanTaskProjectionByStepId[step.id]?.taskId
    }

    func localizedUpgradePlanStatus(_ rawStatus: String) -> String {
        switch rawStatus.lowercased() {
        case "queued":
            return L10n.Service.Task.Status.pending.localized
        case "running":
            return L10n.Service.Task.Status.running.localized
        case "completed":
            return L10n.Service.Task.Status.completed.localized
        case "failed":
            return L10n.Service.Task.Status.failed.localized
        case "cancelled":
            return L10n.Service.Task.Status.cancelled.localized
        default:
            return rawStatus.capitalized
        }
    }

    func upgradeAllPreviewCount(includePinned: Bool = false, allowOsUpdates: Bool = false) -> Int {
        UpgradePreviewPlanner.count(
            candidates: outdatedPackages.map {
                UpgradePreviewPlanner.Candidate(managerId: $0.managerId, pinned: $0.pinned)
            },
            managerEnabled: managerStatuses.mapValues(\.enabled),
            includePinned: includePinned,
            allowOsUpdates: allowOsUpdates,
            safeModeEnabled: safeModeEnabled
        )
    }

    func upgradeAllPreviewBreakdown(
        includePinned: Bool = false,
        allowOsUpdates: Bool = false
    ) -> [(manager: String, count: Int)] {
        UpgradePreviewPlanner.breakdown(
            candidates: outdatedPackages.map {
                UpgradePreviewPlanner.Candidate(managerId: $0.managerId, pinned: $0.pinned)
            },
            managerEnabled: managerStatuses.mapValues(\.enabled),
            includePinned: includePinned,
            allowOsUpdates: allowOsUpdates,
            safeModeEnabled: safeModeEnabled,
            managerName: { [weak self] managerId in
                self?.normalizedManagerName(managerId) ?? managerId
            }
        ).map { (manager: $0.manager, count: $0.count) }
    }

    // MARK: - Localization Helpers

    func localizedUpgradePlanReason(for step: CoreUpgradePlanStep) -> String {
        let args = step.reasonLabelArgs.reduce(into: [String: Any]()) { partialResult, entry in
            partialResult[entry.key] = entry.value
        }
        return step.reasonLabelKey.localized(with: args)
    }

    func localizedUpgradePlanFailureCause(for group: UpgradePlanFailureGroup) -> String {
        L10n.App.Inspector.taskFailureHintGeneric.localized(with: [
            "manager": localizedManagerDisplayName(group.managerId)
        ])
    }

    static func authorityRank(for authority: String) -> Int {
        UpgradePreviewPlanner.authorityRank(for: authority)
    }

    static func sortedUpgradePlanStepsForExecution(_ steps: [CoreUpgradePlanStep]) -> [CoreUpgradePlanStep] {
        let plannerSteps = steps.map {
            UpgradePreviewPlanner.PlanStep(
                id: $0.id,
                orderIndex: $0.orderIndex,
                managerId: $0.managerId,
                authority: $0.authority,
                packageName: $0.packageName,
                reasonLabelKey: $0.reasonLabelKey
            )
        }
        var stepById: [String: CoreUpgradePlanStep] = [:]
        for step in steps where stepById[step.id] == nil {
            stepById[step.id] = step
        }
        return UpgradePreviewPlanner.sortedForExecution(plannerSteps).compactMap { stepById[$0.id] }
    }

    static func scopedUpgradePlanSteps(
        from steps: [CoreUpgradePlanStep],
        managerScopeId: String,
        packageFilter: String
    ) -> [CoreUpgradePlanStep] {
        let plannerSteps = steps.map {
            UpgradePreviewPlanner.PlanStep(
                id: $0.id,
                orderIndex: $0.orderIndex,
                managerId: $0.managerId,
                authority: $0.authority,
                packageName: $0.packageName,
                reasonLabelKey: $0.reasonLabelKey
            )
        }
        var stepById: [String: CoreUpgradePlanStep] = [:]
        for step in steps where stepById[step.id] == nil {
            stepById[step.id] = step
        }
        return UpgradePreviewPlanner.scopedForExecution(
            from: plannerSteps,
            managerScopeId: managerScopeId,
            packageFilter: packageFilter
        ).compactMap { stepById[$0.id] }
    }

    func localizedTaskLabel(from task: CoreTaskRecord) -> String? {
        if let labelKey = task.labelKey {
            let args = task.labelArgs?.reduce(into: [String: Any]()) { partialResult, entry in
                partialResult[entry.key] = entry.value
            } ?? [:]
            return labelKey.localized(with: args)
        }
        // `task.label` is persisted server-side in English; prefer localized fallback composition.
        return nil
    }

    func localizedTaskType(_ rawTaskType: String) -> String {
        switch rawTaskType.lowercased() {
        case "refresh":
            return L10n.Common.refresh.localized
        case "detection":
            return L10n.Common.initializing.localized
        case "search":
            return L10n.App.Dashboard.Status.searchRemote.localized
        case "install":
            return L10n.Common.install.localized
        case "uninstall":
            return L10n.Common.uninstall.localized
        case "upgrade":
            return L10n.Common.update.localized
        case "pin":
            return L10n.App.Packages.Action.pin.localized
        case "unpin":
            return L10n.App.Packages.Action.unpin.localized
        default:
            return rawTaskType.capitalized
        }
    }

    func diagnosticCommandHint(for task: TaskItem) -> String? {
        guard let managerId = task.managerId?.lowercased(),
              let taskType = task.taskType?.lowercased() else {
            return nil
        }

        let packageArg = normalizedCommandArg(task.labelArgs?["package"])
        let toolchainArg = normalizedCommandArg(task.labelArgs?["toolchain"])

        switch managerId {
        case "homebrew_formula":
            return commandForHomebrewFormula(taskType: taskType, packageArg: packageArg)
        case "homebrew_cask":
            return commandForHomebrewCask(taskType: taskType, packageArg: packageArg)
        case "npm":
            return packageManagerCommand(
                taskType: taskType,
                packageArg: packageArg,
                installPrefix: "npm install -g",
                uninstallPrefix: "npm uninstall -g",
                upgradePrefix: "npm update -g"
            )
        case "pnpm":
            return packageManagerCommand(
                taskType: taskType,
                packageArg: packageArg,
                installPrefix: "pnpm add -g",
                uninstallPrefix: "pnpm remove -g",
                upgradePrefix: "pnpm update -g"
            )
        case "yarn":
            return packageManagerCommand(
                taskType: taskType,
                packageArg: packageArg,
                installPrefix: "yarn global add",
                uninstallPrefix: "yarn global remove",
                upgradePrefix: "yarn global upgrade"
            )
        case "pip":
            return commandForPip(taskType: taskType, packageArg: packageArg)
        case "pipx":
            return packageManagerCommand(
                taskType: taskType,
                packageArg: packageArg,
                installPrefix: "pipx install",
                uninstallPrefix: "pipx uninstall",
                upgradePrefix: "pipx upgrade"
            )
        case "rubygems":
            return packageManagerCommand(
                taskType: taskType,
                packageArg: packageArg,
                installPrefix: "gem install",
                uninstallPrefix: "gem uninstall",
                upgradePrefix: "gem update"
            )
        case "poetry":
            return commandForPoetry(taskType: taskType, packageArg: packageArg)
        case "cargo":
            return commandForCargo(taskType: taskType, packageArg: packageArg)
        case "cargo_binstall":
            return commandForCargoBinstall(taskType: taskType, packageArg: packageArg)
        case "rustup":
            return commandForRustup(taskType: taskType, toolchainArg: toolchainArg)
        case "softwareupdate":
            return taskType == "upgrade" ? "softwareupdate --install --all" : nil
        case "mise":
            return commandForMise(taskType: taskType, packageArg: packageArg)
        default:
            return nil
        }
    }

    private func normalizedCommandArg(_ value: String?) -> String? {
        guard let trimmed = value?.trimmingCharacters(in: .whitespacesAndNewlines),
              !trimmed.isEmpty else {
            return nil
        }
        return trimmed
    }

    private func packageManagerCommand(
        taskType: String,
        packageArg: String?,
        installPrefix: String,
        uninstallPrefix: String,
        upgradePrefix: String
    ) -> String? {
        guard let packageArg else { return nil }
        switch taskType {
        case "install":
            return "\(installPrefix) \(packageArg)"
        case "uninstall":
            return "\(uninstallPrefix) \(packageArg)"
        case "upgrade":
            return "\(upgradePrefix) \(packageArg)"
        default:
            return nil
        }
    }

    private func commandForHomebrewFormula(taskType: String, packageArg: String?) -> String? {
        switch taskType {
        case "install":
            guard let packageArg else { return nil }
            return "brew install \(packageArg)"
        case "uninstall":
            guard let packageArg else { return nil }
            return "brew uninstall \(packageArg)"
        case "upgrade":
            if let packageArg {
                return "brew upgrade \(packageArg)"
            }
            return "brew upgrade"
        default:
            return nil
        }
    }

    private func commandForHomebrewCask(taskType: String, packageArg: String?) -> String? {
        guard taskType == "upgrade" else { return nil }
        if let packageArg {
            return "brew upgrade --cask \(packageArg)"
        }
        return "brew upgrade --cask"
    }

    private func commandForPip(taskType: String, packageArg: String?) -> String? {
        guard let packageArg else { return nil }
        switch taskType {
        case "install", "upgrade":
            return "python3 -m pip install --upgrade \(packageArg)"
        case "uninstall":
            return "python3 -m pip uninstall \(packageArg)"
        default:
            return nil
        }
    }

    private func commandForPoetry(taskType: String, packageArg: String?) -> String? {
        switch taskType {
        case "install":
            guard let packageArg else { return nil }
            return "poetry self add \(packageArg)"
        case "uninstall":
            guard let packageArg else { return nil }
            return "poetry self remove \(packageArg)"
        case "upgrade":
            if let packageArg {
                return "poetry self update \(packageArg)"
            }
            return "poetry self update"
        default:
            return nil
        }
    }

    private func commandForCargo(taskType: String, packageArg: String?) -> String? {
        guard let packageArg else { return nil }
        switch taskType {
        case "install", "upgrade":
            return "cargo install \(packageArg)"
        case "uninstall":
            return "cargo uninstall \(packageArg)"
        default:
            return nil
        }
    }

    private func commandForCargoBinstall(taskType: String, packageArg: String?) -> String? {
        guard let packageArg else { return nil }
        switch taskType {
        case "install", "upgrade":
            return "cargo binstall \(packageArg)"
        case "uninstall":
            return "cargo uninstall \(packageArg)"
        default:
            return nil
        }
    }

    private func commandForRustup(taskType: String, toolchainArg: String?) -> String? {
        guard taskType == "upgrade" else { return nil }
        if let toolchainArg {
            return "rustup update \(toolchainArg)"
        }
        return "rustup update"
    }

    private func commandForMise(taskType: String, packageArg: String?) -> String? {
        guard taskType == "upgrade" else { return nil }
        if let packageArg {
            return "mise upgrade \(packageArg)"
        }
        return "mise upgrade"
    }

    func upgradeActionDescription(for package: PackageItem) -> String {
        switch package.managerId {
        case "homebrew_formula":
            if shouldCleanupOldKegs(for: package) {
                return L10n.Service.Task.Label.upgradeHomebrewCleanup.localized(with: ["package": package.name])
            }
            return L10n.Service.Task.Label.upgradeHomebrew.localized(with: ["package": package.name])
        case "mise":
            return L10n.Service.Task.Label.upgradeMise.localized(with: ["package": package.name])
        case "rustup":
            return L10n.Service.Task.Label.upgradeRustupToolchain.localized(with: ["toolchain": package.name])
        default:
            return L10n.Service.Task.Label.upgradePackage.localized(with: [
                "package": package.name,
                "manager": normalizedManagerName(package.managerId)
            ])
        }
    }

    func managerActionDescription(action: String, managerId: String) -> String {
        switch (action, managerId) {
        case ("Update", "homebrew_formula"):
            return L10n.Service.Task.Label.updateHomebrewSelf.localized
        case ("Update", "rustup"):
            return L10n.Service.Task.Label.updateRustupSelf.localized
        case ("Uninstall", "rustup"):
            return L10n.Service.Task.Label.uninstallRustupSelf.localized
        default:
            return L10n.App.Tasks.fallbackDescription.localized(with: [
                "task_type": action,
                "manager": normalizedManagerName(managerId)
            ])
        }
    }

    func shouldCleanupOldKegs(for package: PackageItem) -> Bool {
        if package.managerId != "homebrew_formula" {
            return false
        }

        switch kegPolicySelection(for: package) {
        case .cleanup:
            return true
        case .keep:
            return false
        case .useGlobal:
            return homebrewKegAutoCleanupEnabled
        }
    }
}

private struct HelmCliShimStatus {
    let bundledCliPath: String?
    let shimInstalled: Bool
}

private enum HelmCliShimInstaller {
    struct InstallResult {
        let shimPath: String
    }

    enum Error: Swift.Error {
        case bundledCliMissing
        case existingNonManagedShim(path: String)
        case removeRefusedNonManagedShim(path: String)
        case ioFailure(description: String)

        var logDescription: String {
            switch self {
            case .bundledCliMissing:
                return "bundled CLI binary is missing from the app bundle"
            case .existingNonManagedShim(let path):
                return "refusing to replace non-Helm shim at \(path)"
            case .removeRefusedNonManagedShim(let path):
                return "refusing to remove non-Helm shim at \(path)"
            case .ioFailure(let description):
                return description
            }
        }
    }

    private static let shimSentinel = "# helm-cli-shim: app-bundle"
    private static let defaultBundleIdentifier = "app.jasoncavinder.Helm"

    static func status(bundle: Bundle, shimURL: URL) -> HelmCliShimStatus {
        let bundledCliPath = bundledCliURL(bundle: bundle)?.path
        let shimInstalled = isManagedShimInstalled(shimURL: shimURL)
        return HelmCliShimStatus(
            bundledCliPath: bundledCliPath,
            shimInstalled: shimInstalled
        )
    }

    static func install(bundle: Bundle, shimURL: URL, markerURL: URL) throws -> InstallResult {
        guard let bundledCliURL = bundledCliURL(bundle: bundle) else {
            throw Error.bundledCliMissing
        }

        if FileManager.default.fileExists(atPath: shimURL.path),
           !isManagedShimInstalled(shimURL: shimURL) {
            throw Error.existingNonManagedShim(path: shimURL.path)
        }

        let bundlePath = bundle.bundleURL.standardizedFileURL.path
        let bundleIdentifier = bundle.bundleIdentifier ?? defaultBundleIdentifier
        let script = renderShimScript(
            appBundlePath: bundlePath,
            appBundleIdentifier: bundleIdentifier
        )

        do {
            try writeTextAtomically(
                script,
                to: shimURL,
                filePermissions: 0o755,
                disallowSymlinkTarget: false
            )
            let quarantineCleared = try removeQuarantineAttributeIfPresent(at: shimURL)
            try writeInstallMarker(
                markerURL: markerURL,
                version: bundleVersion(bundle: bundle)
            )
            logger.info(
                "Helm CLI shim post-install attributes. shim=\(shimURL.path, privacy: .public), quarantine_cleared=\(quarantineCleared, privacy: .public)"
            )
        } catch {
            throw Error.ioFailure(description: error.localizedDescription)
        }

        logger.info(
            "Helm CLI shim installed. shim=\(shimURL.path, privacy: .public), bundled_cli=\(bundledCliURL.path, privacy: .public)"
        )
        return InstallResult(shimPath: shimURL.path)
    }

    static func remove(shimURL: URL, markerURL: URL) throws -> Bool {
        var removedShim = false
        guard FileManager.default.fileExists(atPath: shimURL.path) else {
            try? removeInstallMarkerIfAppBundleShim(markerURL: markerURL)
            return false
        }
        guard isManagedShimInstalled(shimURL: shimURL) else {
            throw Error.removeRefusedNonManagedShim(path: shimURL.path)
        }

        do {
            try FileManager.default.removeItem(at: shimURL)
            removedShim = true
            try? removeInstallMarkerIfAppBundleShim(markerURL: markerURL)
        } catch {
            throw Error.ioFailure(description: error.localizedDescription)
        }
        return removedShim
    }

    private static func bundledCliURL(bundle: Bundle) -> URL? {
        guard let bundledCliURL = bundle.url(
            forResource: "helm-cli",
            withExtension: nil
        ) else {
            return nil
        }
        guard FileManager.default.isExecutableFile(atPath: bundledCliURL.path) else {
            return nil
        }
        return bundledCliURL
    }

    private static func bundleVersion(bundle: Bundle) -> String {
        if let version = bundle.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String,
           !version.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return version
        }
        return helmVersion
    }

    private static func isManagedShimInstalled(shimURL: URL) -> Bool {
        guard let data = try? Data(contentsOf: shimURL),
              let script = String(data: data, encoding: .utf8) else {
            return false
        }
        return script.contains(shimSentinel)
    }

    private static func shellSingleQuote(_ value: String) -> String {
        let escaped = value.replacingOccurrences(of: "'", with: "'\"'\"'")
        return "'\(escaped)'"
    }

    private static func renderShimScript(
        appBundlePath: String,
        appBundleIdentifier: String
    ) -> String {
        let quotedBundlePath = shellSingleQuote(appBundlePath)
        let quotedBundleIdentifier = shellSingleQuote(appBundleIdentifier)
        return """
        #!/bin/sh
        set -eu

        HELM_APP_BUNDLE_PATH=\(quotedBundlePath)
        HELM_APP_BUNDLE_ID=\(quotedBundleIdentifier)
        HELM_CLI_RELATIVE_PATH='Contents/Resources/helm-cli'
        HELM_SHIM_SELF="$0"
        \(shimSentinel)

        resolve_cli_from_bundle() {
          bundle_path="$1"
          candidate="${bundle_path%/}/${HELM_CLI_RELATIVE_PATH}"
          if [ -x "$candidate" ]; then
            printf '%s\\n' "$candidate"
            return 0
          fi
          return 1
        }

        resolve_cli() {
          if candidate="$(resolve_cli_from_bundle "$HELM_APP_BUNDLE_PATH")"; then
            printf '%s\\n' "$candidate"
            return 0
          fi

          if command -v osascript >/dev/null 2>&1; then
            discovered="$(osascript -e 'try' -e "POSIX path of (path to application id \\"${HELM_APP_BUNDLE_ID}\\")" -e 'on error' -e 'return ""' -e 'end try' 2>/dev/null | tr -d '\\r')"
            if [ -n "$discovered" ]; then
              if candidate="$(resolve_cli_from_bundle "${discovered%/}")"; then
                printf '%s\\n' "$candidate"
                return 0
              fi
            fi
          fi

          return 1
        }

        if cli_path="$(resolve_cli)"; then
          exec "$cli_path" "$@"
        fi

        echo "Helm app bundle was not found or is missing its embedded CLI binary." >&2
        echo "If Helm is removed, this shim can remove itself." >&2
        echo "Shim path: ${HELM_SHIM_SELF}" >&2

        if [ -t 0 ]; then
          printf "Remove this shim now? [y/N] " >&2
          if read -r answer; then
            case "$answer" in
              y|Y|yes|YES)
                if rm -f -- "$HELM_SHIM_SELF"; then
                  echo "Removed shim: ${HELM_SHIM_SELF}" >&2
                else
                  echo "Failed to remove shim: ${HELM_SHIM_SELF}" >&2
                fi
                ;;
            esac
          fi
        fi

        exit 1
        """
    }

    private static func writeInstallMarker(markerURL: URL, version: String) throws {
        struct InstallMarker: Codable {
            let channel: String
            let artifact: String
            let installedAt: String
            let updatePolicy: String
            let version: String

            enum CodingKeys: String, CodingKey {
                case channel
                case artifact
                case installedAt = "installed_at"
                case updatePolicy = "update_policy"
                case version
            }
        }

        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime]
        let payload = InstallMarker(
            channel: "app-bundle-shim",
            artifact: "helm-cli",
            installedAt: formatter.string(from: Date()),
            updatePolicy: "channel",
            version: version
        )
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted]
        let data = try encoder.encode(payload)
        guard let json = String(data: data, encoding: .utf8) else {
            throw CocoaError(.fileReadInapplicableStringEncoding)
        }
        try writeTextAtomically(
            json + "\n",
            to: markerURL,
            filePermissions: 0o644,
            disallowSymlinkTarget: true
        )
    }

    @discardableResult
    private static func removeQuarantineAttributeIfPresent(at url: URL) throws -> Bool {
        let attributeName = "com.apple.quarantine"
        let result = url.path.withCString { pathPointer in
            attributeName.withCString { attributePointer in
                removexattr(pathPointer, attributePointer, 0)
            }
        }
        if result == 0 {
            return true
        }

        let errorCode = errno
        if errorCode == ENOATTR {
            return false
        }

        let errorMessage = String(cString: strerror(errorCode))
        throw NSError(
            domain: "HelmCliShimInstaller",
            code: Int(errorCode),
            userInfo: [
                NSLocalizedDescriptionKey:
                    "Failed to clear \(attributeName) from \(url.path): \(errorMessage)"
            ]
        )
    }

    private static func removeInstallMarkerIfAppBundleShim(markerURL: URL) throws {
        let fileManager = FileManager.default
        guard fileManager.fileExists(atPath: markerURL.path) else {
            return
        }
        guard let markerData = try? Data(contentsOf: markerURL),
              let markerText = String(data: markerData, encoding: .utf8),
              markerText.contains("\"channel\": \"app-bundle-shim\"") else {
            return
        }
        try fileManager.removeItem(at: markerURL)
    }

    private static func writeTextAtomically(
        _ content: String,
        to url: URL,
        filePermissions: Int,
        disallowSymlinkTarget: Bool
    ) throws {
        let fileManager = FileManager.default
        let parentDirectory = url.deletingLastPathComponent()
        try fileManager.createDirectory(
            at: parentDirectory,
            withIntermediateDirectories: true
        )

        if disallowSymlinkTarget,
           fileManager.fileExists(atPath: url.path) {
            let values = try url.resourceValues(forKeys: [.isSymbolicLinkKey])
            if values.isSymbolicLink == true {
                throw NSError(
                    domain: "HelmCliShimInstaller",
                    code: 1,
                    userInfo: [NSLocalizedDescriptionKey: "Refusing to write through symlink path: \(url.path)"]
                )
            }
        }

        let tempURL = parentDirectory.appendingPathComponent(".\(url.lastPathComponent).tmp-\(UUID().uuidString)")
        do {
            guard let encoded = content.data(using: .utf8) else {
                throw NSError(
                    domain: "HelmCliShimInstaller",
                    code: 2,
                    userInfo: [NSLocalizedDescriptionKey: "Failed to encode text content as UTF-8"]
                )
            }
            try encoded.write(to: tempURL, options: .withoutOverwriting)
            try fileManager.setAttributes(
                [.posixPermissions: NSNumber(value: filePermissions)],
                ofItemAtPath: tempURL.path
            )
            if fileManager.fileExists(atPath: url.path) {
                _ = try fileManager.replaceItemAt(
                    url,
                    withItemAt: tempURL,
                    backupItemName: nil,
                    options: []
                )
            } else {
                try fileManager.moveItem(at: tempURL, to: url)
            }
        } catch {
            try? fileManager.removeItem(at: tempURL)
            throw error
        }
    }
}

// MARK: - Support & Diagnostics

struct HelmSupport {
    static let supportEmail = "jason.cavinder+helm@gmail.com"
    static let licenseTermsURL = URL(string: "https://github.com/jasoncavinder/Helm/blob/main/LICENSE")!
    static let gitHubSponsorsURL = URL(string: "https://github.com/sponsors/jasoncavinder")!
    static let gitHubNewIssueURL = URL(string: "https://github.com/jasoncavinder/Helm/issues/new")!
    static let gitHubBugReportURL = URL(string: "https://github.com/jasoncavinder/Helm/issues/new?template=bug_report.yml")!
    static let gitHubFeatureRequestURL = URL(string: "https://github.com/jasoncavinder/Helm/issues/new?template=feature_request.yml")!
    static let patreonURL = URL(string: "https://www.patreon.com/cw/jasoncavinder")!
    static let buyMeACoffeeURL = URL(string: "https://buymeacoffee.com/jasoncavinder")!
    static let koFiURL = URL(string: "https://ko-fi.com/jasoncavinder")!
    static let payPalURL = URL(string: "https://paypal.me/jasoncavinder")!
    static let venmoURL = URL(string: "https://www.venmo.com/u/JasonCavinder")!

    private struct SupportExportPayload: Codable {
        let schemaVersion: String
        let generatedAt: String
        let app: SupportExportAppContext
        let system: SupportExportSystemContext
        let lastError: String?
        let lastErrorAttribution: SupportExportErrorAttribution?
        let managers: [SupportExportManagerContext]
        let tasks: [SupportExportTaskContext]
        let failures: [SupportExportFailureContext]
        let redaction: SupportExportRedactionContext
    }

    private struct SupportExportAppContext: Codable {
        let version: String
        let locale: String
        let distributionChannel: String
        let safeModeEnabled: Bool
        let managerCount: Int
        let outdatedCount: Int
        let runningTaskCount: Int
    }

    private struct SupportExportSystemContext: Codable {
        let macOSVersion: String
        let architecture: String
    }

    private struct SupportExportManagerContext: Codable {
        let id: String
        let displayName: String
        let enabled: Bool
        let detected: Bool
        let version: String?
        let executablePath: String?
        let authority: String
    }

    private struct SupportExportTaskContext: Codable {
        let id: String
        let status: String
        let managerId: String?
        let taskType: String?
        let description: String
        let labelKey: String?
        let labelArgs: [String: String]?
    }

    private struct SupportExportFailureContext: Codable {
        let taskId: String
        let managerId: String?
        let taskType: String?
        let status: String
        let description: String
        let suggestedCommand: String?
    }

    private struct SupportExportRedactionContext: Codable {
        let appliedRules: [String]
        let replacementCount: Int
    }

    private struct SupportExportErrorAttribution: Codable {
        let source: String
        let action: String
        let managerId: String?
        let taskType: String?
        let occurredAtUnix: Int64
    }

    struct FeedbackBody {
        let type: String
        let description: String
        let reproduction: String
        let managers: String
        let diagnostics: String
        
        func toString() -> String {
            return """
            Feedback Type: \(type)
            
            Description:
            \(description)
            
            Steps to Reproduce (if applicable):
            \(reproduction)
            
            Managers Involved:
            \(managers)
            
            Diagnostics:
            \(diagnostics)
            """
        }
    }

    private static func machineArchitecture() -> String {
        var sysInfo = utsname()
        uname(&sysInfo)
        return withUnsafeBytes(of: &sysInfo.machine) { buf in
            guard let baseAddress = buf.baseAddress else { return "" }
            return String(cString: baseAddress.assumingMemoryBound(to: CChar.self))
        }
    }

    private static func buildStructuredDiagnosticsPayload() -> SupportExportPayload {
        let core = HelmCore.shared
        var redactor = SupportRedactor()

        let sortedManagers = core.managerStatuses.map { id, status in
            let manager = ManagerInfo.find(byId: id)
            let authorityRank: Int
            switch manager?.authority {
            case .authoritative:
                authorityRank = 0
            case .standard, .none:
                authorityRank = 1
            case .guarded:
                authorityRank = 2
            }
            return (
                id: id,
                status: status,
                authorityRank: authorityRank,
                sortName: manager?.displayName ?? id,
                authorityName: manager?.authority.key.localized ?? "standard"
            )
        }.sorted { lhs, rhs in
            if lhs.authorityRank != rhs.authorityRank {
                return lhs.authorityRank < rhs.authorityRank
            }
            return lhs.sortName.localizedCaseInsensitiveCompare(rhs.sortName) == .orderedAscending
        }

        let managerSnapshots = sortedManagers.map { entry in
            SupportExportManagerContext(
                id: entry.id,
                displayName: localizedManagerDisplayName(entry.id),
                enabled: entry.status.enabled,
                detected: entry.status.detected,
                version: redactor.redactOptionalString(entry.status.version),
                executablePath: redactor.redactOptionalString(entry.status.executablePath),
                authority: entry.authorityName
            )
        }

        let taskSnapshots = core.activeTasks.map { task in
            SupportExportTaskContext(
                id: task.id,
                status: task.status,
                managerId: task.managerId,
                taskType: task.taskType,
                description: redactor.redactString(task.description),
                labelKey: task.labelKey,
                labelArgs: redactor.redactDictionary(task.labelArgs)
            )
        }

        let failureSnapshots = core.activeTasks
            .filter { $0.status.lowercased() == "failed" }
            .map { task in
                SupportExportFailureContext(
                    taskId: task.id,
                    managerId: task.managerId,
                    taskType: task.taskType,
                    status: task.status,
                    description: redactor.redactString(task.description),
                    suggestedCommand: redactor.redactOptionalString(core.diagnosticCommandHint(for: task))
                )
            }

        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]

        return SupportExportPayload(
            schemaVersion: "1.0.0",
            generatedAt: formatter.string(from: Date()),
            app: SupportExportAppContext(
                version: helmVersion,
                locale: Locale.current.identifier,
                distributionChannel: AppUpdateConfiguration.from().channel.rawValue,
                safeModeEnabled: core.safeModeEnabled,
                managerCount: core.visibleManagers.count,
                outdatedCount: core.outdatedPackages.count,
                runningTaskCount: core.runningTaskCount
            ),
            system: SupportExportSystemContext(
                macOSVersion: ProcessInfo.processInfo.operatingSystemVersionString,
                architecture: machineArchitecture()
            ),
            lastError: redactor.redactOptionalString(core.lastError),
            lastErrorAttribution: core.lastErrorAttribution.map { attribution in
                SupportExportErrorAttribution(
                    source: attribution.source,
                    action: attribution.action,
                    managerId: attribution.managerId,
                    taskType: attribution.taskType,
                    occurredAtUnix: attribution.occurredAtUnix
                )
            },
            managers: managerSnapshots,
            tasks: taskSnapshots,
            failures: failureSnapshots,
            redaction: SupportExportRedactionContext(
                appliedRules: redactor.appliedRules.sorted(),
                replacementCount: redactor.replacementCount
            )
        )
    }

    static func generateStructuredDiagnostics() -> String {
        let payload = buildStructuredDiagnosticsPayload()
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]

        guard let data = try? encoder.encode(payload),
              let json = String(data: data, encoding: .utf8) else {
            return "{}"
        }
        return json
    }

    static func generateDiagnostics() -> String {
        var info = ""
        info += "Helm Version: \(helmVersion)\n"
        info += "macOS Version: \(ProcessInfo.processInfo.operatingSystemVersionString)\n"
        
        var sysInfo = utsname()
        uname(&sysInfo)
        let machine = withUnsafeBytes(of: &sysInfo.machine) { buf in
             guard let baseAddress = buf.baseAddress else { return "" }
             return String(cString: baseAddress.assumingMemoryBound(to: CChar.self))
        }
        info += "Architecture: \(machine)\n"
        info += "Locale: \(Locale.current.identifier)\n"
        
        info += "\nManagers:\n"
        let core = HelmCore.shared
        let sortedManagers = core.managerStatuses.map { id, status in
            let manager = ManagerInfo.find(byId: id)
            let authorityRank: Int
            switch manager?.authority {
            case .authoritative:
                authorityRank = 0
            case .standard, .none:
                authorityRank = 1
            case .guarded:
                authorityRank = 2
            }
            return (
                id: id,
                status: status,
                authorityRank: authorityRank,
                sortName: manager?.displayName ?? id
            )
        }.sorted { lhs, rhs in
            if lhs.authorityRank != rhs.authorityRank {
                return lhs.authorityRank < rhs.authorityRank
            }
            return lhs.sortName.localizedCaseInsensitiveCompare(rhs.sortName) == .orderedAscending
        }

        for entry in sortedManagers {
            let id = entry.id
            let status = entry.status
            let state = status.enabled ? "Enabled" : "Disabled"
            let installed = status.detected ? "Installed" : "Not Detected"
            let version = status.version ?? "Unknown"
            info += "- \(id): \(state), \(installed) (v\(version))\n"
        }
        
        info += "\nRecent Tasks:\n"
        if core.activeTasks.isEmpty {
            info += "(No active tasks)\n"
        } else {
            for task in core.activeTasks {
                info += "- [\(task.id)] \(task.description) (\(task.status))"
                if let mid = task.managerId {
                    info += " [\(mid)]"
                }
                info += "\n"
            }
        }
        if let lastError = core.lastError, !lastError.isEmpty {
            info += "\nLast Error: \(lastError)\n"
        }
        if let attribution = core.lastErrorAttribution {
            info += "Last Error Source: \(attribution.source)\n"
            info += "Last Error Action: \(attribution.action)\n"
            if let managerId = attribution.managerId, !managerId.isEmpty {
                info += "Last Error Manager: \(managerId)\n"
            }
            if let taskType = attribution.taskType, !taskType.isEmpty {
                info += "Last Error Task Type: \(taskType)\n"
            }
        }
        
        return info
    }

    static func copyDiagnosticsToClipboard() {
        let diagnostics = generateDiagnostics()
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(diagnostics, forType: .string)
    }

    private static func serviceHealthManagerCounts(
        core: HelmCore
    ) -> (enabled: Int, detected: Int, missing: Int) {
        let trackedStatuses = core.managerStatuses.values
            .filter { $0.isImplemented && $0.enabled }
        let enabled = trackedStatuses.count
        let detected = trackedStatuses.filter(\.detected).count
        return (enabled, detected, max(enabled - detected, 0))
    }

    static func generateServiceHealthDiagnostics() -> String {
        let core = HelmCore.shared
        let appUpdate = AppUpdateCoordinator.shared
        let isoFormatter = ISO8601DateFormatter()
        isoFormatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        let managerCounts = serviceHealthManagerCounts(core: core)

        var info = ""
        info += "Service Health Snapshot\n"
        info += "Generated: \(isoFormatter.string(from: Date()))\n"
        info += "Helm Version: \(helmVersion)\n"
        info += "Connection: \(core.isConnected ? "Connected" : "Disconnected")\n"
        info += "Refresh State: \(core.isRefreshing ? "Refreshing" : "Idle")\n"
        info += "Aggregate Health: \(core.aggregateHealth.key.localized)\n"
        if let lastCheckDate = appUpdate.lastCheckDate {
            info += "Last Check: \(isoFormatter.string(from: lastCheckDate))\n"
        } else {
            info += "Last Check: Never\n"
        }
        info += "Running Tasks: \(core.runningTaskCount)\n"
        info += "Failed Tasks: \(core.failedTaskCount)\n"
        info += "Pending Updates: \(core.outdatedPackages.count)\n"
        info += "Detected Managers: \(managerCounts.detected)/\(managerCounts.enabled)\n"
        info += "Managers Missing: \(managerCounts.missing)\n"
        if let lastError = core.lastError, !lastError.isEmpty {
            info += "Last Error: \(lastError)\n"
        }
        if let attribution = core.lastErrorAttribution {
            info += "Last Error Source: \(attribution.source)\n"
            info += "Last Error Action: \(attribution.action)\n"
            if let managerId = attribution.managerId, !managerId.isEmpty {
                info += "Last Error Manager: \(managerId)\n"
            }
            if let taskType = attribution.taskType, !taskType.isEmpty {
                info += "Last Error Task Type: \(taskType)\n"
            }
        }
        return info
    }

    static func copyServiceHealthDiagnosticsToClipboard() {
        let snapshot = generateServiceHealthDiagnostics()
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(snapshot, forType: .string)
    }

    static func copyStructuredDiagnosticsToClipboard() {
        let diagnostics = generateStructuredDiagnostics()
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(diagnostics, forType: .string)
    }

    static func redactForStructuredExport(_ raw: String) -> String {
        var redactor = SupportRedactor()
        return redactor.redactString(raw)
    }

    static func generateTaskDiagnostics(
        task: TaskItem,
        suggestedCommand: String?,
        output: CoreTaskOutputRecord? = nil
    ) -> String {
        var info = generateDiagnostics()
        info += "\nTask Focus:\n"
        info += "- Task ID: \(task.id)\n"
        info += "- Status: \(task.status)\n"
        if let managerId = task.managerId {
            info += "- Manager: \(managerId)\n"
        }
        if let taskType = task.taskType {
            info += "- Task Type: \(taskType)\n"
        }
        if let labelKey = task.labelKey {
            info += "- Label Key: \(labelKey)\n"
        }
        if let labelArgs = task.labelArgs, !labelArgs.isEmpty {
            info += "- Label Args:\n"
            for entry in labelArgs.sorted(by: { $0.key < $1.key }) {
                info += "  - \(entry.key): \(entry.value)\n"
            }
        }
        if let suggestedCommand, !suggestedCommand.isEmpty {
            info += "- Suggested Repro Command: \(suggestedCommand)\n"
        }
        if let output {
            if let command = output.command, !command.isEmpty {
                info += "- Command: \(command)\n"
            }
            if let cwd = output.cwd, !cwd.isEmpty {
                info += "- CWD: \(cwd)\n"
            }
            if let programPath = output.programPath, !programPath.isEmpty {
                info += "- Program Path: \(programPath)\n"
            }
            if let pathSnippet = output.pathSnippet, !pathSnippet.isEmpty {
                info += "- PATH Snippet: \(pathSnippet)\n"
            }
            if let startedAtUnixMs = output.startedAtUnixMs {
                info += "- Started At (unix ms): \(startedAtUnixMs)\n"
            }
            if let finishedAtUnixMs = output.finishedAtUnixMs {
                info += "- Finished At (unix ms): \(finishedAtUnixMs)\n"
            }
            if let durationMs = output.durationMs {
                info += "- Duration (ms): \(durationMs)\n"
            }
            if let exitCode = output.exitCode {
                info += "- Exit Code: \(exitCode)\n"
            }
            if let terminationReason = output.terminationReason, !terminationReason.isEmpty {
                info += "- Termination Reason: \(terminationReason)\n"
            }
            if let errorCode = output.errorCode, !errorCode.isEmpty {
                info += "- Error Code: \(errorCode)\n"
            }
            if let errorMessage = output.errorMessage, !errorMessage.isEmpty {
                info += "- Error Message: \(errorMessage)\n"
            }
        }
        return info
    }

    static func copyTaskDiagnosticsToClipboard(task: TaskItem, suggestedCommand: String?) {
        let diagnostics = generateTaskDiagnostics(task: task, suggestedCommand: suggestedCommand)
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(diagnostics, forType: .string)
    }
    
    static func openURL(_ url: URL) {
        NSWorkspace.shared.open(url)
    }

    static func emailFeedback() {
        let subject = "Helm Feedback (v\(helmVersion))"
        let body = FeedbackBody(
            type: "Bug / Feature / UX / Other",
            description: "(What happened / what you expected)",
            reproduction: "(Steps to reproduce)",
            managers: "(If applicable)",
            diagnostics: "(Paste diagnostics here if relevant)"
        ).toString()
        
        var components = URLComponents()
        components.scheme = "mailto"
        components.path = supportEmail
        components.queryItems = [
            URLQueryItem(name: "subject", value: subject),
            URLQueryItem(name: "body", value: body)
        ]
        
        if let url = components.url {
            NSWorkspace.shared.open(url)
        }
    }
    
    static func openGitHubIssue(title: String? = nil, body: String? = nil, includeDiagnostics: Bool = false) {
        var components = URLComponents(url: gitHubNewIssueURL, resolvingAgainstBaseURL: true)
        var queryItems = components?.queryItems ?? []
        if let title = title {
            queryItems.append(URLQueryItem(name: "title", value: title))
        }
        
        var finalBody = body ?? ""
        if includeDiagnostics {
            finalBody += "\n\n```\n\(generateDiagnostics())\n```"
        }
        
        if !finalBody.isEmpty {
            queryItems.append(URLQueryItem(name: "body", value: finalBody))
        }
        
        components?.queryItems = queryItems
        
        if let url = components?.url {
            NSWorkspace.shared.open(url)
        }
    }

    static func reportBug(includeDiagnostics: Bool = false) {
        if includeDiagnostics {
            copyStructuredDiagnosticsToClipboard()
        }
        var components = URLComponents(url: gitHubBugReportURL, resolvingAgainstBaseURL: true)
        var queryItems = components?.queryItems ?? []
        queryItems.append(URLQueryItem(name: "title", value: "[Bug]: "))
        components?.queryItems = queryItems
        if let url = components?.url {
            NSWorkspace.shared.open(url)
        }
    }

    static func requestFeature(includeDiagnostics: Bool = false) {
        if includeDiagnostics {
            copyStructuredDiagnosticsToClipboard()
        }
        var components = URLComponents(url: gitHubFeatureRequestURL, resolvingAgainstBaseURL: true)
        var queryItems = components?.queryItems ?? []
        queryItems.append(URLQueryItem(name: "title", value: "[Feature]: "))
        components?.queryItems = queryItems
        if let url = components?.url {
            NSWorkspace.shared.open(url)
        }
    }
}
