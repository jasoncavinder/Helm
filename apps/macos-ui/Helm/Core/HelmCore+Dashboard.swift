import Foundation

extension HelmCore {
    var allKnownPackages: [PackageItem] {
        let outdatedIds = Set(outdatedPackages.map(\.id))
        var combined = outdatedPackages
        combined.append(contentsOf: installedPackages.filter { !outdatedIds.contains($0.id) })

        let existing = Set(combined.map(\.id))
        combined.append(contentsOf: cachedAvailablePackages.filter { !existing.contains($0.id) })

        return combined
            .sorted { lhs, rhs in
                lhs.name.localizedCaseInsensitiveCompare(rhs.name) == .orderedAscending
            }
    }

    var visibleManagers: [ManagerInfo] {
        ManagerInfo.implemented.filter { manager in
            let status = managerStatuses[manager.id]
            let enabled = status?.enabled ?? true
            let detected = status?.detected ?? false
            return enabled && detected
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

    /// Manager IDs that should show upgrade action badges.
    /// Includes managers with outdated packages, plus softwareupdate when safe mode blocks it.
    var upgradeActionManagerIds: [String] {
        var managerIds = Set(outdatedPackages.map(\.managerId))
        if safeModeEnabled {
            managerIds.insert("softwareupdate")
        }
        return Array(managerIds)
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
            }
            let localIds = Set(localMatches.map(\.id))
            let remoteMatches = searchResults.filter { !localIds.contains($0.id) }
            base = (localMatches + remoteMatches)
                .sorted { lhs, rhs in
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
}
