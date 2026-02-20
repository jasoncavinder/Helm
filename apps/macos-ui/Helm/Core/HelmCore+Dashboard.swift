import Foundation
import os.log

private let taskSyncLogger = Logger(subsystem: "com.jasoncavinder.Helm", category: "core.tasks")

extension HelmCore {
    var allKnownPackages: [PackageItem] {
        let outdatedIds = Set(outdatedPackages.map(\.id))
        let installedOnly = installedPackages.filter { !outdatedIds.contains($0.id) }
        var combined = outdatedPackages + installedOnly

        let cachedById = cachedAvailablePackages.reduce(into: [String: PackageItem]()) { partial, package in
            if var existing = partial[package.id] {
                mergeSummary(into: &existing, from: package.summary)
                partial[package.id] = existing
            } else {
                partial[package.id] = package
            }
        }

        for index in combined.indices {
            if let cached = cachedById[combined[index].id] {
                mergeSummary(into: &combined[index], from: cached.summary)
            }
        }

        let existing = Set(combined.map(\.id))
        combined.append(contentsOf: cachedById.values.filter { !existing.contains($0.id) })

        return combined
            .sorted { lhs, rhs in
                lhs.name.localizedCaseInsensitiveCompare(rhs.name) == .orderedAscending
            }
    }

    var visibleManagers: [ManagerInfo] {
        ManagerInfo.all.filter { manager in
            let status = managerStatuses[manager.id]
            let isImplemented = status?.isImplemented ?? manager.isImplemented
            let enabled = status?.enabled ?? true
            let detected = status?.detected ?? false
            return isImplemented && enabled && detected
        }
    }

    var failedTaskCount: Int {
        activeTasks.filter { $0.status.lowercased() == "failed" }.count
    }

    var runningTaskCount: Int {
        activeTasks.filter(\.isRunning).count
    }

    var aggregateHealth: OperationalHealth {
        if failedTaskCount > 0 {
            return .error
        }
        if runningTaskCount > 0 || isRefreshing {
            return .running
        }
        if !outdatedPackages.isEmpty {
            return .attention
        }
        return .healthy
    }

    /// Returns a filtered and deduplicated package list.
    /// Merges local matches with remote search results (deduped by ID),
    /// then applies optional manager and status filters.
    func filteredPackages(
        query: String,
        managerId: String?,
        statusFilter: PackageStatus?
    ) -> [PackageItem] {
        var base = allKnownPackages
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()

        if !trimmed.isEmpty {
            let localMatches = base.filter {
                $0.name.lowercased().contains(trimmed)
                    || $0.manager.lowercased().contains(trimmed)
                    || ($0.summary?.lowercased().contains(trimmed) ?? false)
            }
            var mergedById = Dictionary(uniqueKeysWithValues: localMatches.map { ($0.id, $0) })
            for remote in searchResults {
                if var existing = mergedById[remote.id] {
                    mergeSummary(into: &existing, from: remote.summary)
                    if existing.latestVersion == nil {
                        existing.latestVersion = remote.latestVersion
                    }
                    mergedById[remote.id] = existing
                } else {
                    mergedById[remote.id] = remote
                }
            }

            base = mergedById.values.sorted { lhs, rhs in
                lhs.name.localizedCaseInsensitiveCompare(rhs.name) == .orderedAscending
            }
        }

        if let managerId {
            base = base.filter { $0.managerId == managerId }
        }

        if let statusFilter {
            base = base.filter { package in
                if package.status == statusFilter {
                    return true
                }
                if statusFilter == .installed && package.status == .upgradable {
                    return true
                }
                return false
            }
        }

        return base
    }

    func outdatedCount(forManagerId managerId: String) -> Int {
        outdatedPackages.filter { $0.managerId == managerId }.count
    }

    func upgradeAllPackages(forManagerId managerId: String) {
        let packages = outdatedPackages.filter {
            $0.managerId == managerId && !$0.pinned && canUpgradeIndividually($0)
        }
        for package in packages {
            upgradePackage(package)
        }
    }

    func health(forManagerId managerId: String) -> OperationalHealth {
        if let status = managerStatuses[managerId], status.detected == false {
            return .notInstalled
        }
        if managerStatuses[managerId] == nil && !detectedManagers.contains(managerId) {
            return .notInstalled
        }

        let hasFailedTask = activeTasks.contains {
            $0.status.lowercased() == "failed" && $0.managerId == managerId
        }

        if hasFailedTask {
            return .error
        }
        if activeTasks.contains(where: {
            $0.isRunning && $0.managerId == managerId
        }) {
            return .running
        }
        if outdatedPackages.contains(where: { $0.managerId == managerId }) {
            return .attention
        }
        return .healthy
    }

    func canUpgradeIndividually(_ package: PackageItem) -> Bool {
        return package.status == .upgradable
            && (managerStatuses[package.managerId]?.supportsPackageUpgrade ?? false)
            && !package.pinned
            && isManagerEnabled(package.managerId)
    }

    func canInstallPackage(_ package: PackageItem) -> Bool {
        package.status == .available
            && (managerStatuses[package.managerId]?.supportsPackageInstall ?? false)
            && isManagerEnabled(package.managerId)
    }

    func canUninstallPackage(_ package: PackageItem) -> Bool {
        package.status != .available
            && (managerStatuses[package.managerId]?.supportsPackageUninstall ?? false)
            && isManagerEnabled(package.managerId)
    }

    func canPinPackage(_ package: PackageItem) -> Bool {
        package.status != .available && package.managerId == "homebrew_formula"
    }

    func supportsRemoteSearch(managerId: String) -> Bool {
        managerStatuses[managerId]?.supportsRemoteSearch ?? false
    }

    func isManagerEnabled(_ managerId: String) -> Bool {
        managerStatuses[managerId]?.enabled ?? true
    }

    func syncManagerOperations(from coreTasks: [CoreTaskRecord]) {
        let statusById = Dictionary(uniqueKeysWithValues: coreTasks.map { ($0.id, $0.status.lowercased()) })
        let inFlightStates = Set(["queued", "running"])

        for managerId in Array(managerActionTaskByManager.keys) {
            guard let taskId = managerActionTaskByManager[managerId] else { continue }
            guard let status = statusById[taskId] else {
                managerOperations.removeValue(forKey: managerId)
                managerActionTaskByManager.removeValue(forKey: managerId)
                continue
            }
            if !inFlightStates.contains(status) {
                managerOperations.removeValue(forKey: managerId)
                managerActionTaskByManager.removeValue(forKey: managerId)
            }
        }
    }

    func syncUpgradeActions(from coreTasks: [CoreTaskRecord]) {
        let statusById = Dictionary(uniqueKeysWithValues: coreTasks.map { ($0.id, $0.status.lowercased()) })
        let inFlightStates = Set(["queued", "running"])
        var shouldRefreshSnapshots = false

        for packageId in Array(upgradeActionTaskByPackage.keys) {
            guard let taskId = upgradeActionTaskByPackage[packageId] else { continue }
            guard let status = statusById[taskId] else {
                upgradeActionTaskByPackage.removeValue(forKey: packageId)
                upgradeActionPackageIds.remove(packageId)
                continue
            }
            if inFlightStates.contains(status) {
                continue
            }

            upgradeActionTaskByPackage.removeValue(forKey: packageId)
            upgradeActionPackageIds.remove(packageId)
            if status == "completed" {
                shouldRefreshSnapshots = true
            }
        }

        if shouldRefreshSnapshots {
            fetchPackages()
            fetchOutdatedPackages()
        }
    }

    func syncInstallActions(from coreTasks: [CoreTaskRecord]) {
        let statusById = Dictionary(uniqueKeysWithValues: coreTasks.map { ($0.id, $0.status.lowercased()) })
        let inFlightStates = Set(["queued", "running"])
        var shouldRefreshSnapshots = false

        for packageId in Array(installActionTaskByPackage.keys) {
            guard let taskId = installActionTaskByPackage[packageId] else { continue }
            guard let status = statusById[taskId] else {
                installActionTaskByPackage.removeValue(forKey: packageId)
                installActionPackageIds.remove(packageId)
                continue
            }
            if inFlightStates.contains(status) {
                continue
            }

            installActionTaskByPackage.removeValue(forKey: packageId)
            installActionPackageIds.remove(packageId)
            if status == "completed" {
                shouldRefreshSnapshots = true
            }
        }

        if shouldRefreshSnapshots {
            fetchPackages()
            fetchOutdatedPackages()
            refreshCachedAvailablePackages()
        }
    }

    func syncUninstallActions(from coreTasks: [CoreTaskRecord]) {
        let statusById = Dictionary(uniqueKeysWithValues: coreTasks.map { ($0.id, $0.status.lowercased()) })
        let inFlightStates = Set(["queued", "running"])
        var shouldRefreshSnapshots = false

        for packageId in Array(uninstallActionTaskByPackage.keys) {
            guard let taskId = uninstallActionTaskByPackage[packageId] else { continue }
            guard let status = statusById[taskId] else {
                uninstallActionTaskByPackage.removeValue(forKey: packageId)
                uninstallActionPackageIds.remove(packageId)
                continue
            }
            if inFlightStates.contains(status) {
                continue
            }

            uninstallActionTaskByPackage.removeValue(forKey: packageId)
            uninstallActionPackageIds.remove(packageId)
            if status == "completed" {
                shouldRefreshSnapshots = true
            }
        }

        if shouldRefreshSnapshots {
            fetchPackages()
            fetchOutdatedPackages()
            refreshCachedAvailablePackages()
        }
    }

    func syncPackageDescriptionLookups(from coreTasks: [CoreTaskRecord]) {
        let statusById = Dictionary(uniqueKeysWithValues: coreTasks.map { ($0.id, $0.status.lowercased()) })
        let inFlightStates = Set(["queued", "running"])

        for packageId in Array(descriptionLookupTaskIdsByPackage.keys) {
            guard let taskIds = descriptionLookupTaskIdsByPackage[packageId], !taskIds.isEmpty else {
                descriptionLookupTaskIdsByPackage.removeValue(forKey: packageId)
                packageDescriptionLoadingIds.remove(packageId)
                continue
            }

            let inFlightTaskIds = taskIds.filter { taskId in
                guard let status = statusById[taskId] else { return false }
                return inFlightStates.contains(status)
            }

            if !inFlightTaskIds.isEmpty {
                descriptionLookupTaskIdsByPackage[packageId] = inFlightTaskIds
                continue
            }

            descriptionLookupTaskIdsByPackage.removeValue(forKey: packageId)
            packageDescriptionLoadingIds.remove(packageId)

            let summary = allKnownPackages
                .first(where: { $0.id == packageId })?
                .summary?
                .trimmingCharacters(in: .whitespacesAndNewlines)

            if summary?.isEmpty == false {
                packageDescriptionUnavailableIds.remove(packageId)
            } else {
                packageDescriptionUnavailableIds.insert(packageId)
            }
        }
    }

    func updateOnboardingDetectionProgress(from coreTasks: [CoreTaskRecord]) {
        guard onboardingDetectionInProgress else { return }

        let terminalStatuses = Set(["completed", "failed", "cancelled"])
        for task in coreTasks
        where task.id > onboardingDetectionAnchorTaskId
            && task.taskType.lowercased() == "detection"
            && terminalStatuses.contains(task.status.lowercased())
        {
            onboardingDetectionPendingManagers.remove(task.manager)
        }

        pruneOnboardingDetectionForDisabledManagers()

        if onboardingDetectionPendingManagers.isEmpty {
            completeOnboardingDetectionProgress()
            return
        }

        if let startedAt = onboardingDetectionStartedAt,
           Date().timeIntervalSince(startedAt) > 90
        {
            let pending = onboardingDetectionPendingManagers.joined(separator: ",")
            taskSyncLogger.warning("Onboarding detection timed out waiting for managers: \(pending)")
            completeOnboardingDetectionProgress()
        }
    }

    private func mergeSummary(into package: inout PackageItem, from candidate: String?) {
        let existingSummary = package.summary?.trimmingCharacters(in: .whitespacesAndNewlines)
        guard existingSummary?.isEmpty != false else { return }
        guard let candidate else { return }
        let trimmedCandidate = candidate.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedCandidate.isEmpty else { return }
        package.summary = trimmedCandidate
    }
}
