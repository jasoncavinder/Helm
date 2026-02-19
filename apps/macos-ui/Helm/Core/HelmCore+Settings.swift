import Foundation
import os.log

private let logger = Logger(subsystem: "com.jasoncavinder.Helm", category: "core.settings")

extension HelmCore {

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
        service()?.upgradeAll(includePinned: includePinned, allowOsUpdates: allowOsUpdates) { success in
            if !success {
                logger.error("upgradeAll(includePinned: \(includePinned), allowOsUpdates: \(allowOsUpdates)) failed")
            }
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
