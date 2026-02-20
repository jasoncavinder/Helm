import Foundation

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

    private func mergeSummary(into package: inout PackageItem, from candidate: String?) {
        let existingSummary = package.summary?.trimmingCharacters(in: .whitespacesAndNewlines)
        guard existingSummary?.isEmpty != false else { return }
        guard let candidate else { return }
        let trimmedCandidate = candidate.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedCandidate.isEmpty else { return }
        package.summary = trimmedCandidate
    }
}
