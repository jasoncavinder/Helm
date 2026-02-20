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
    static let patreonURL = URL(string: "https://patreon.com/jasoncavinder")!
    
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
        for (id, status) in core.managerStatuses {
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
            copyDiagnosticsToClipboard()
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
            copyDiagnosticsToClipboard()
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
