import AppKit
import Foundation
import os.log
import Darwin

private let logger = Logger(subsystem: "com.jasoncavinder.Helm", category: "core.settings")

extension HelmCore {
    static let allManagersScopeId = UpgradePreviewPlanner.allManagersScopeId

    // MARK: - Safe Mode

    func fetchSafeMode() {
        service()?.getSafeMode { [weak self] enabled in
            DispatchQueue.main.async {
                self?.safeModeEnabled = enabled
            }
        }
    }

    func setSafeMode(_ enabled: Bool) {
        service()?.setSafeMode(enabled: enabled) { [weak self] success in
            DispatchQueue.main.async {
                if success {
                    self?.safeModeEnabled = enabled
                } else {
                    logger.error("setSafeMode(\(enabled)) failed")
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
        service()?.setHomebrewKegAutoCleanup(enabled: enabled) { [weak self] success in
            DispatchQueue.main.async {
                if success {
                    self?.homebrewKegAutoCleanupEnabled = enabled
                } else {
                    logger.error("setHomebrewKegAutoCleanup(\(enabled)) failed")
                }
            }
        }
    }

    // MARK: - Keg Policies

    func fetchPackageKegPolicies() {
        service()?.listPackageKegPolicies { [weak self] jsonString in
            guard let jsonString = jsonString, let data = jsonString.data(using: .utf8) else { return }

            do {
                let decoder = JSONDecoder()
                decoder.keyDecodingStrategy = .convertFromSnakeCase
                let entries = try decoder.decode([CorePackageKegPolicy].self, from: data)

                DispatchQueue.main.async {
                    var overrides: [String: HomebrewKegPolicyOverride] = [:]
                    for entry in entries where entry.managerId == "homebrew_formula" {
                        overrides["\(entry.managerId):\(entry.packageName)"] = entry.policy
                    }
                    self?.packageKegPolicyOverrides = overrides
                }
            } catch {
                logger.error("Failed to decode package keg policies: \(error)")
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

        service()?.setPackageKegPolicy(managerId: package.managerId, packageName: package.name, policyMode: policyMode) { [weak self] success in
            DispatchQueue.main.async {
                guard let self = self else { return }
                guard success else {
                    logger.error("setPackageKegPolicy(\(package.managerId):\(package.name), \(policyMode)) failed")
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
        service()?.upgradeAll(includePinned: includePinned, allowOsUpdates: allowOsUpdates) { success in
            if !success {
                logger.error("upgradeAll(includePinned: \(includePinned), allowOsUpdates: \(allowOsUpdates)) failed")
            }
        }
    }

    func refreshUpgradePlan(includePinned: Bool = false, allowOsUpdates: Bool = false) {
        service()?.previewUpgradePlan(includePinned: includePinned, allowOsUpdates: allowOsUpdates) { [weak self] jsonString in
            guard let self = self else { return }
            guard let jsonString, let data = jsonString.data(using: .utf8) else { return }

            do {
                let decoder = JSONDecoder()
                decoder.keyDecodingStrategy = .convertFromSnakeCase
                let steps = try decoder.decode([CoreUpgradePlanStep].self, from: data)
                DispatchQueue.main.async {
                    self.upgradePlanIncludePinned = includePinned
                    self.upgradePlanAllowOsUpdates = allowOsUpdates
                    self.upgradePlanSteps = steps.sorted { lhs, rhs in
                        lhs.orderIndex < rhs.orderIndex
                    }
                    self.syncUpgradePlanProjection(from: self.latestCoreTasksSnapshot)
                }
            } catch {
                logger.error("refreshUpgradePlan: decode failed (\(data.count) bytes): \(error)")
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
        case ("Install", "mas"):
            return L10n.Service.Task.Label.installHomebrewFormula.localized(with: ["package": "mas"])
        case ("Install", "mise"):
            return L10n.Service.Task.Label.installHomebrewFormula.localized(with: ["package": "mise"])
        case ("Update", "homebrew_formula"):
            return L10n.Service.Task.Label.updateHomebrewSelf.localized
        case ("Update", "mas"):
            return L10n.Service.Task.Label.updateHomebrewFormula.localized(with: ["package": "mas"])
        case ("Update", "mise"):
            return L10n.Service.Task.Label.updateHomebrewFormula.localized(with: ["package": "mise"])
        case ("Update", "rustup"):
            return L10n.Service.Task.Label.updateRustupSelf.localized
        case ("Uninstall", "mas"):
            return L10n.Service.Task.Label.uninstallHomebrewFormula.localized(with: ["package": "mas"])
        case ("Uninstall", "mise"):
            return L10n.Service.Task.Label.uninstallHomebrewFormula.localized(with: ["package": "mise"])
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

// MARK: - Support & Diagnostics

struct HelmSupport {
    static let supportEmail = "jason.cavinder+helm@gmail.com"
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

    private struct SupportRedactor {
        private(set) var appliedRules: Set<String> = []
        private(set) var replacementCount: Int = 0
        private let homeDirectory = NSHomeDirectory()

        mutating func redactString(_ raw: String) -> String {
            var value = raw
            value = applyLiteral(
                rule: "home_directory",
                value: value,
                target: homeDirectory,
                replacement: "~"
            )
            value = applyRegex(
                rule: "user_path",
                value: value,
                pattern: #"/Users/[^/\s]+"#,
                replacement: "/Users/[redacted-user]"
            )
            value = applyRegex(
                rule: "email",
                value: value,
                pattern: #"(?i)\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b"#,
                replacement: "[redacted-email]"
            )
            value = applyRegex(
                rule: "github_token",
                value: value,
                pattern: #"\b(gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,})\b"#,
                replacement: "[redacted-token]"
            )
            return value
        }

        mutating func redactOptionalString(_ raw: String?) -> String? {
            guard let raw else { return nil }
            return redactString(raw)
        }

        mutating func redactDictionary(_ raw: [String: String]?) -> [String: String]? {
            guard let raw else { return nil }
            var redacted: [String: String] = [:]
            for (key, value) in raw {
                redacted[key] = redactString(value)
            }
            return redacted
        }

        private mutating func applyLiteral(
            rule: String,
            value: String,
            target: String,
            replacement: String
        ) -> String {
            guard !target.isEmpty else { return value }
            let count = value.components(separatedBy: target).count - 1
            guard count > 0 else { return value }
            appliedRules.insert(rule)
            replacementCount += count
            return value.replacingOccurrences(of: target, with: replacement)
        }

        private mutating func applyRegex(
            rule: String,
            value: String,
            pattern: String,
            replacement: String
        ) -> String {
            guard let regex = try? NSRegularExpression(pattern: pattern) else {
                return value
            }
            let range = NSRange(value.startIndex..<value.endIndex, in: value)
            let matches = regex.numberOfMatches(in: value, range: range)
            guard matches > 0 else { return value }
            appliedRules.insert(rule)
            replacementCount += matches
            return regex.stringByReplacingMatches(in: value, range: range, withTemplate: replacement)
        }
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

    static func generateTaskDiagnostics(task: TaskItem, suggestedCommand: String?) -> String {
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
