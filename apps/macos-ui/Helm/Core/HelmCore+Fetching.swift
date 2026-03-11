import AppKit
import Foundation

extension HelmCore {

    func fetchPackages() {
        guard let svc = service() else { return }
        withTimeout(
            30,
            source: "core.fetching",
            action: "listInstalledPackages",
            taskType: "refresh",
            operation: { completion in
            svc.listInstalledPackages { completion($0) }
        }) { [weak self] jsonString in
            guard let self = self,
                  let jsonString = jsonString,
                  let data = jsonString.data(using: .utf8),
                  let corePackages: [CoreInstalledPackage] = self.decodeCorePayload(
                    [CoreInstalledPackage].self,
                    from: data,
                    decodeContext: "fetchPackages",
                    source: "core.fetching",
                    action: "listInstalledPackages.decode",
                    taskType: "refresh"
                  ) else { return }

            DispatchQueue.main.async {
                self.installedPackages = corePackages.map { pkg in
                    PackageItem(
                        id: self.availablePackageId(
                            managerId: pkg.package.manager,
                            packageName: pkg.package.name,
                            packageIdentifier: pkg.packageIdentifier,
                            version: pkg.installedVersion
                        ),
                        name: pkg.package.name,
                        packageIdentifier: pkg.packageIdentifier,
                        version: pkg.installedVersion ?? L10n.Common.unknown.localized,
                        managerId: pkg.package.manager,
                        manager: self.normalizedManagerName(pkg.package.manager),
                        pinned: pkg.pinned,
                        runtimeState: pkg.runtimeState ?? PackageRuntimeState()
                    )
                }
            }
        }
    }

    func fetchOutdatedPackages() {
        guard let svc = service() else { return }
        withTimeout(
            30,
            source: "core.fetching",
            action: "listOutdatedPackages",
            taskType: "refresh",
            operation: { completion in
            svc.listOutdatedPackages { completion($0) }
        }) { [weak self] jsonString in
            guard let self = self,
                  let jsonString = jsonString,
                  let data = jsonString.data(using: .utf8),
                  let corePackages: [CoreOutdatedPackage] = self.decodeCorePayload(
                    [CoreOutdatedPackage].self,
                    from: data,
                    decodeContext: "fetchOutdatedPackages",
                    source: "core.fetching",
                    action: "listOutdatedPackages.decode",
                    taskType: "refresh"
                  ) else { return }

            DispatchQueue.main.async {
                self.outdatedPackages = corePackages.map { pkg in
                    PackageItem(
                        id: self.availablePackageId(
                            managerId: pkg.package.manager,
                            packageName: pkg.package.name,
                            packageIdentifier: pkg.packageIdentifier,
                            version: pkg.installedVersion
                        ),
                        name: pkg.package.name,
                        packageIdentifier: pkg.packageIdentifier,
                        version: pkg.installedVersion ?? L10n.Common.unknown.localized,
                        latestVersion: pkg.candidateVersion,
                        managerId: pkg.package.manager,
                        manager: self.normalizedManagerName(pkg.package.manager),
                        pinned: pkg.pinned,
                        restartRequired: pkg.restartRequired,
                        runtimeState: pkg.runtimeState ?? PackageRuntimeState()
                    )
                }
                if !self.upgradePlanSteps.isEmpty {
                    self.refreshUpgradePlan(
                        includePinned: self.upgradePlanIncludePinned,
                        allowOsUpdates: self.upgradePlanAllowOsUpdates
                    )
                }
            }
        }
    }

    func fetchTasks() {
        guard let svc = service() else { return }
        withTimeout(
            30,
            source: "core.fetching",
            action: "listTasks",
            taskType: "refresh",
            operation: { completion in
            svc.listTasks { completion($0) }
        }) { [weak self] jsonString in
            guard let self = self,
                  let jsonString = jsonString,
                  let data = jsonString.data(using: .utf8),
                  let coreTasks: [CoreTaskRecord] = self.decodeCorePayload(
                    [CoreTaskRecord].self,
                    from: data,
                    decodeContext: "fetchTasks",
                    source: "core.fetching",
                    action: "listTasks.decode",
                    taskType: "refresh"
                  ) else { return }

            DispatchQueue.main.async {
                self.latestCoreTasksSnapshot = coreTasks
                if let maxTaskId = coreTasks.map(\.id).max() {
                    self.lastObservedTaskId = max(self.lastObservedTaskId, maxTaskId)
                }
                let coreTaskIds = Set(coreTasks.map(\.id))

                let previousFailed = self.previousFailedTaskCount

                let coreTaskItems = coreTasks.map { task in
                    let overrideDescription = self.managerActionTaskDescriptions[task.id]
                    let managerName = self.normalizedManagerName(task.manager)
                    let taskLabel = self.localizedTaskLabel(from: task)
                    return TaskItem(
                        id: "\(task.id)",
                        description: taskLabel
                            ?? overrideDescription
                            ?? L10n.App.Tasks.fallbackDescription.localized(with: [
                                "task_type": self.localizedTaskType(task.taskType),
                                "manager": managerName
                            ]),
                        status: task.status.capitalized,
                        managerId: task.manager,
                        taskType: task.taskType,
                        labelKey: task.labelKey,
                        labelArgs: task.labelArgs
                    )
                }
                self.syncManagerOperations(from: coreTasks)
                self.syncManagerPostInstallSetupTasks(from: coreTasks)
                self.syncUpgradeActions(from: coreTasks)
                self.syncInstallActions(from: coreTasks)
                self.syncUninstallActions(from: coreTasks)
                self.syncRustupToolchainActions(from: coreTasks)
                self.syncUpgradePlanProjection(from: coreTasks)
                self.syncPackageDescriptionLookups(from: coreTasks)
                self.activeTasks = (
                    coreTaskItems
                    + self.pendingManagerActionPlaceholderTasks(excluding: coreTaskIds)
                    + self.pendingLocalManagerActionTasks()
                )
                .sorted { $0.statusSortOrder < $1.statusSortOrder }

                // Announce new task failures to VoiceOver
                let currentFailed = self.activeTasks.filter({ $0.status.lowercased() == "failed" }).count
                if currentFailed > previousFailed {
                    let newFailures = currentFailed - previousFailed
                    self.postAccessibilityAnnouncement(
                        "app.status_item.error".localized(with: ["count": newFailures])
                    )
                }
                self.previousFailedTaskCount = currentFailed

                // Detection truth should come from manager status payloads when available.
                // Task terminal state alone ("completed") does not imply "detected = true".
                if self.managerStatuses.isEmpty {
                    // Startup fallback while manager status has not been fetched yet.
                    var latestDetectionByManager: [String: String] = [:]
                    for task in coreTasks {
                        guard task.taskType.lowercased() == "detection" else { continue }
                        if latestDetectionByManager[task.manager] == nil {
                            latestDetectionByManager[task.manager] = task.status.lowercased()
                        }
                    }
                    var detected = Set<String>()
                    for (manager, status) in latestDetectionByManager where status == "completed" {
                        detected.insert(manager)
                    }
                    self.detectedManagers = detected
                } else {
                    self.detectedManagers = Set(
                        self.managerStatuses.compactMap { entry in
                            entry.value.detected ? entry.key : nil
                        }
                    )
                }
                self.updateOnboardingDetectionProgress(from: coreTasks)

                let isRunning = coreTasks.contains {
                    let type = $0.taskType.lowercased()
                    let status = $0.status.lowercased()
                    return (type == "refresh" || type == "detection") &&
                           (status == "running" || status == "queued")
                }

                // Only show "refreshing" when we triggered a refresh this session.
                // Without this guard, stale running tasks from a previous session
                // would permanently lock isRefreshing = true.
                let wasRefreshing = self.previousRefreshState
                if let lastTrigger = self.lastRefreshTrigger {
                    if Date().timeIntervalSince(lastTrigger) > 120.0 {
                        // Safety valve: clear stuck refresh after 2 minutes
                        self.isRefreshing = false
                        self.lastRefreshTrigger = nil
                    } else if isRunning {
                        self.isRefreshing = true
                    } else if Date().timeIntervalSince(lastTrigger) < 2.0 {
                        self.isRefreshing = true
                    } else {
                        self.isRefreshing = false
                        self.lastRefreshTrigger = nil
                    }
                } else {
                    self.isRefreshing = false
                }

                // Announce refresh completion to VoiceOver
                let nowRefreshing = self.isRefreshing
                if wasRefreshing && !nowRefreshing {
                    self.postAccessibilityAnnouncement(
                        L10n.Common.success.localized
                    )
                }
                self.previousRefreshState = nowRefreshing

                let inFlightSearchTaskIds = Set(
                    coreTasks.compactMap { task -> Int64? in
                        let taskType = task.taskType.lowercased()
                        let status = task.status.lowercased()
                        guard taskType == "search",
                              status == "queued" || status == "running" else {
                            return nil
                        }
                        return Int64(task.id)
                    }
                )
                let inFlightInteractiveSearchTaskIds = Set(
                    coreTasks.compactMap { task -> Int64? in
                        let taskType = task.taskType.lowercased()
                        let status = task.status.lowercased()
                        guard taskType == "search",
                              status == "queued" || status == "running" else {
                            return nil
                        }
                        let query = task.labelArgs?["query"]?.trimmingCharacters(in: .whitespacesAndNewlines)
                        guard let query, !query.isEmpty else { return nil }
                        return Int64(task.id)
                    }
                )
                self.activeRemoteSearchTaskIds = inFlightSearchTaskIds
                let hasQuery = !self.searchText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                self.isSearching = hasQuery && !inFlightInteractiveSearchTaskIds.isEmpty
            }
        }
    }

    private func pendingManagerActionPlaceholderTasks(excluding coreTaskIds: Set<UInt64>) -> [TaskItem] {
        let now = Date()
        return managerActionTaskByManager.compactMap { managerId, taskId in
            guard !coreTaskIds.contains(taskId) else { return nil }
            guard let submittedAt = managerActionTaskSubmittedAt[taskId] else { return nil }
            guard now.timeIntervalSince(submittedAt) < Self.managerActionTaskMissingGraceSeconds else {
                return nil
            }

            let trackedType = managerActionTaskTypes[taskId]?
                .trimmingCharacters(in: .whitespacesAndNewlines)
                .lowercased()
            let action = managerActionLabelAction(for: trackedType)
            let description = managerActionTaskDescriptions[taskId]
                ?? managerActionDescription(action: action, managerId: managerId)

            return TaskItem(
                id: "\(taskId)",
                description: description,
                status: "Queued",
                managerId: managerId,
                taskType: managerActionPlaceholderTaskType(for: trackedType),
                labelKey: nil,
                labelArgs: nil
            )
        }
    }

    private func pendingLocalManagerActionTasks() -> [TaskItem] {
        let now = Date()
        let expiredTaskIds = localManagerActionTaskCreatedAt.compactMap { taskId, createdAt in
            now.timeIntervalSince(createdAt) >= Self.localManagerActionTaskRetentionSeconds ? taskId : nil
        }
        for taskId in expiredTaskIds {
            localManagerActionTaskCreatedAt.removeValue(forKey: taskId)
            localManagerActionTasks.removeValue(forKey: taskId)
        }
        return localManagerActionTasks.values.map { $0 }
    }

    private func managerActionLabelAction(for trackedType: String?) -> String {
        switch trackedType {
        case "manager_install":
            return "Install"
        case "manager_uninstall":
            return "Uninstall"
        default:
            return "Update"
        }
    }

    private func managerActionPlaceholderTaskType(for trackedType: String?) -> String? {
        switch trackedType {
        case "manager_install":
            return "install"
        case "manager_uninstall":
            return "uninstall"
        case "manager_update":
            return "upgrade"
        default:
            return nil
        }
    }

    func fetchSearchResults(query: String) {
        let normalizedQuery = query.trimmingCharacters(in: .whitespacesAndNewlines)
        localSearchRequestGeneration &+= 1
        let requestGeneration = localSearchRequestGeneration

        guard !normalizedQuery.isEmpty else {
            DispatchQueue.main.async {
                guard self.localSearchRequestGeneration == requestGeneration else { return }
                self.searchResults = []
            }
            return
        }

        service()?.searchLocal(query: normalizedQuery) { [weak self] jsonString in
            guard let self = self,
                  let jsonString = jsonString,
                  let data = jsonString.data(using: .utf8),
                  let results: [CoreSearchResult] = self.decodeCorePayload(
                    [CoreSearchResult].self,
                    from: data,
                    decodeContext: "fetchSearchResults",
                    source: "core.fetching",
                    action: "searchLocal.decode",
                    taskType: "search"
                  ) else {
                DispatchQueue.main.async {
                    guard let self, self.localSearchRequestGeneration == requestGeneration else { return }
                    self.searchResults = []
                }
                return
            }

            DispatchQueue.main.async {
                guard self.localSearchRequestGeneration == requestGeneration else { return }
                self.mergePackageDescriptionSummaryIndex(from: results)
                var installedOrOutdatedRepresentativeIdsByIdentity: [String: String] = [:]
                var installedOrOutdatedRepresentativeIdsByStableKey: [String: String] = [:]
                for package in (self.outdatedPackages + self.installedPackages) {
                    let identityId = self.packageIdentityId(
                        managerId: package.managerId,
                        packageName: package.name,
                        packageIdentifier: package.packageIdentifier,
                        version: package.version
                    )
                    if installedOrOutdatedRepresentativeIdsByIdentity[identityId] == nil {
                        installedOrOutdatedRepresentativeIdsByIdentity[identityId] = package.id
                    }
                    if package.managerId == "asdf" {
                        let stableKey = self.packageStableLookupKey(
                            managerId: package.managerId,
                            packageName: package.name
                        )
                        if installedOrOutdatedRepresentativeIdsByStableKey[stableKey] == nil {
                            installedOrOutdatedRepresentativeIdsByStableKey[stableKey] = package.id
                        }
                    }
                }
                let resolvedSummaryIds = Set(
                    results.compactMap { result -> String? in
                        guard let summary = result.summary?.trimmingCharacters(in: .whitespacesAndNewlines),
                              !summary.isEmpty else {
                            return nil
                        }
                        let identityId = self.packageIdentityId(
                            managerId: result.sourceManager,
                            packageName: result.name,
                            packageIdentifier: result.packageIdentifier,
                            version: result.version
                        )
                        if let representativeId = installedOrOutdatedRepresentativeIdsByIdentity[identityId] {
                            return representativeId
                        }
                        if result.sourceManager == "asdf",
                           let representativeId = installedOrOutdatedRepresentativeIdsByStableKey[
                            self.packageStableLookupKey(
                                managerId: result.sourceManager,
                                packageName: result.name
                            )
                           ] {
                            return representativeId
                        }
                        return self.availablePackageId(
                            managerId: result.sourceManager,
                            packageName: result.name,
                            packageIdentifier: result.packageIdentifier,
                            version: result.version
                        )
                    }
                )

                self.searchResults = results.map { result in
                    let identityId = self.packageIdentityId(
                        managerId: result.sourceManager,
                        packageName: result.name,
                        packageIdentifier: result.packageIdentifier,
                        version: result.version
                    )
                    let packageId = installedOrOutdatedRepresentativeIdsByIdentity[identityId]
                        ?? (
                            result.sourceManager == "asdf"
                                ? installedOrOutdatedRepresentativeIdsByStableKey[
                                    self.packageStableLookupKey(
                                        managerId: result.sourceManager,
                                        packageName: result.name
                                    )
                                  ]
                                : nil
                        )
                        ?? self.availablePackageId(
                            managerId: result.sourceManager,
                            packageName: result.name,
                            packageIdentifier: result.packageIdentifier,
                            version: result.version
                        )
                    return PackageItem(
                        id: packageId,
                        name: result.name,
                        packageIdentifier: result.packageIdentifier,
                        version: result.version ?? "",
                        managerId: result.sourceManager,
                        manager: self.normalizedManagerName(result.sourceManager),
                        summary: result.summary,
                        status: .available
                    )
                }
                self.packageDescriptionUnavailableIds.subtract(resolvedSummaryIds)
                self.packageDescriptionLoadingIds.subtract(resolvedSummaryIds)
                self.reconcilePackageDescriptionLookupState()
            }
        }
    }

    func refreshCachedAvailablePackages() {
        service()?.searchLocal(query: "") { [weak self] jsonString in
            guard let self = self,
                  let jsonString = jsonString,
                  let data = jsonString.data(using: .utf8),
                  let results: [CoreSearchResult] = self.decodeCorePayload(
                    [CoreSearchResult].self,
                    from: data,
                    decodeContext: "refreshCachedAvailablePackages",
                    source: "core.fetching",
                    action: "refreshCachedAvailablePackages.decode",
                    taskType: "search"
                  ) else { return }

            DispatchQueue.main.async {
                self.rebuildPackageDescriptionSummaryIndex(from: results)
                let excludedIdentityIds = Set(
                    self.installedPackages.map { package in
                        self.packageIdentityId(
                            managerId: package.managerId,
                            packageName: package.name,
                            packageIdentifier: package.packageIdentifier,
                            version: package.version
                        )
                    }
                )
                .union(
                    self.outdatedPackages.map { package in
                        self.packageIdentityId(
                            managerId: package.managerId,
                            packageName: package.name,
                            packageIdentifier: package.packageIdentifier,
                            version: package.version
                        )
                    }
                )
                let excludedAsdfStableKeys = Set(
                    (self.installedPackages + self.outdatedPackages)
                        .filter { $0.managerId == "asdf" }
                        .map { package in
                            self.packageStableLookupKey(
                                managerId: package.managerId,
                                packageName: package.name
                            )
                        }
                )
                var dedupedById: [String: PackageItem] = [:]

                for result in results {
                    let identityId = self.packageIdentityId(
                        managerId: result.sourceManager,
                        packageName: result.name,
                        packageIdentifier: result.packageIdentifier,
                        version: result.version
                    )
                    guard !excludedIdentityIds.contains(identityId) else { continue }
                    if result.sourceManager == "asdf",
                       excludedAsdfStableKeys.contains(
                        self.packageStableLookupKey(
                            managerId: result.sourceManager,
                            packageName: result.name
                        )
                       ) {
                        continue
                    }

                    let id = self.availablePackageId(
                        managerId: result.sourceManager,
                        packageName: result.name,
                        packageIdentifier: result.packageIdentifier,
                        version: result.version
                    )
                    let candidate = PackageItem(
                        id: id,
                        name: result.name,
                        packageIdentifier: result.packageIdentifier,
                        version: result.version ?? "",
                        managerId: result.sourceManager,
                        manager: self.normalizedManagerName(result.sourceManager),
                        summary: result.summary,
                        status: .available
                    )

                    if var existing = dedupedById[id] {
                        let existingSummary = existing.summary?.trimmingCharacters(in: .whitespacesAndNewlines)
                        if existingSummary?.isEmpty != false,
                           let candidateSummary = candidate.summary?.trimmingCharacters(in: .whitespacesAndNewlines),
                           !candidateSummary.isEmpty {
                            existing.summary = candidateSummary
                        }
                        dedupedById[id] = existing
                    } else {
                        dedupedById[id] = candidate
                    }
                }

                self.cachedAvailablePackages = dedupedById.values.sorted { lhs, rhs in
                    lhs.displayName.localizedCaseInsensitiveCompare(rhs.displayName) == .orderedAscending
                }
                let resolvedSummaryIds = Set(
                    self.cachedAvailablePackages.compactMap { package -> String? in
                        guard let summary = package.summary?.trimmingCharacters(in: .whitespacesAndNewlines),
                              !summary.isEmpty else {
                            return nil
                        }
                        return package.id
                    }
                )
                self.packageDescriptionUnavailableIds.subtract(resolvedSummaryIds)
                self.packageDescriptionLoadingIds.subtract(resolvedSummaryIds)
                self.reconcilePackageDescriptionLookupState()
            }
        }
    }

    private func rebuildPackageDescriptionSummaryIndex(from results: [CoreSearchResult]) {
        var next: [String: String] = [:]
        for result in results {
            guard let summary = normalizedDescriptionSummary(result.summary) else { continue }
            let sourceKey = packageDescriptionLookupKey(
                managerId: result.sourceManager,
                packageName: result.name,
                version: result.version
            )
            next[sourceKey] = summary

            if result.manager != result.sourceManager {
                let managerKey = packageDescriptionLookupKey(
                    managerId: result.manager,
                    packageName: result.name,
                    version: result.version
                )
                next[managerKey] = summary
            }
        }
        packageDescriptionSummaryByKey = next
    }

    private func mergePackageDescriptionSummaryIndex(from results: [CoreSearchResult]) {
        var merged = packageDescriptionSummaryByKey
        var changed = false
        for result in results {
            guard let summary = normalizedDescriptionSummary(result.summary) else { continue }
            let sourceKey = packageDescriptionLookupKey(
                managerId: result.sourceManager,
                packageName: result.name,
                version: result.version
            )
            if merged[sourceKey] != summary {
                merged[sourceKey] = summary
                changed = true
            }

            if result.manager != result.sourceManager {
                let managerKey = packageDescriptionLookupKey(
                    managerId: result.manager,
                    packageName: result.name,
                    version: result.version
                )
                if merged[managerKey] != summary {
                    merged[managerKey] = summary
                    changed = true
                }
            }
        }
        if changed {
            packageDescriptionSummaryByKey = merged
        }
    }

    private func normalizedDescriptionSummary(_ summary: String?) -> String? {
        guard let summary else { return nil }
        let trimmed = summary.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        return trimmed
    }

    private func reconcilePackageDescriptionLookupState() {
        let trackedPackageIds = Set(descriptionLookupPackageById.keys)
            .union(packageDescriptionLoadingIds)
            .union(packageDescriptionUnavailableIds)
        for packageId in trackedPackageIds {
            if hasPackageDescriptionSummary(packageId: packageId) {
                packageDescriptionUnavailableIds.remove(packageId)
                if descriptionLookupTaskIdsByPackage[packageId]?.isEmpty != false {
                    packageDescriptionLoadingIds.remove(packageId)
                    descriptionLookupStartedAtByPackage.removeValue(forKey: packageId)
                }
            }
        }
    }

    func refreshPackageDescriptionSummaryFromLocalCache(
        for package: PackageItem,
        clearTracking: Bool = true
    ) {
        guard let service = service() else {
            if hasPackageDescriptionSummary(packageId: package.id) {
                packageDescriptionUnavailableIds.remove(package.id)
                packageDescriptionLoadingIds.remove(package.id)
            } else if clearTracking {
                packageDescriptionUnavailableIds.insert(package.id)
                packageDescriptionLoadingIds.remove(package.id)
            } else {
                packageDescriptionUnavailableIds.remove(package.id)
                packageDescriptionLoadingIds.insert(package.id)
            }
            if clearTracking || hasPackageDescriptionSummary(packageId: package.id) {
                descriptionLookupTaskIdsByPackage.removeValue(forKey: package.id)
                descriptionLookupStartedAtByPackage.removeValue(forKey: package.id)
                descriptionLookupPackageById.removeValue(forKey: package.id)
            }
            return
        }

        service.searchLocal(query: package.name) { [weak self] jsonString in
            guard let self = self else { return }

            guard let jsonString,
                  let data = jsonString.data(using: .utf8),
                  let results: [CoreSearchResult] = self.decodeCorePayload(
                    [CoreSearchResult].self,
                    from: data,
                    decodeContext: "refreshPackageDescriptionSummaryFromLocalCache",
                    source: "core.fetching",
                    action: "refreshPackageDescriptionSummaryFromLocalCache.decode",
                    taskType: "search"
                  ) else {
                DispatchQueue.main.async {
                    if self.hasPackageDescriptionSummary(packageId: package.id) {
                        self.packageDescriptionUnavailableIds.remove(package.id)
                        self.packageDescriptionLoadingIds.remove(package.id)
                    } else if clearTracking {
                        self.packageDescriptionUnavailableIds.insert(package.id)
                        self.packageDescriptionLoadingIds.remove(package.id)
                    } else {
                        self.packageDescriptionUnavailableIds.remove(package.id)
                        self.packageDescriptionLoadingIds.insert(package.id)
                    }
                    if clearTracking || self.hasPackageDescriptionSummary(packageId: package.id) {
                        self.descriptionLookupTaskIdsByPackage.removeValue(forKey: package.id)
                        self.descriptionLookupStartedAtByPackage.removeValue(forKey: package.id)
                        self.descriptionLookupPackageById.removeValue(forKey: package.id)
                    }
                }
                return
            }

            DispatchQueue.main.async {
                self.mergePackageDescriptionSummaryIndex(from: results)
                if self.hasPackageDescriptionSummary(packageId: package.id) {
                    self.packageDescriptionUnavailableIds.remove(package.id)
                    self.packageDescriptionLoadingIds.remove(package.id)
                    self.descriptionLookupTaskIdsByPackage.removeValue(forKey: package.id)
                    self.descriptionLookupStartedAtByPackage.removeValue(forKey: package.id)
                    self.descriptionLookupPackageById.removeValue(forKey: package.id)
                } else if clearTracking {
                    self.packageDescriptionUnavailableIds.insert(package.id)
                    self.packageDescriptionLoadingIds.remove(package.id)
                    self.descriptionLookupTaskIdsByPackage.removeValue(forKey: package.id)
                    self.descriptionLookupStartedAtByPackage.removeValue(forKey: package.id)
                    self.descriptionLookupPackageById.removeValue(forKey: package.id)
                } else {
                    self.packageDescriptionUnavailableIds.remove(package.id)
                    self.packageDescriptionLoadingIds.insert(package.id)
                }
            }
        }
    }

    func rustupToolchainDetail(for package: PackageItem) -> CoreRustupToolchainDetail? {
        guard let key = rustupToolchainDetailKey(for: package) else { return nil }
        return rustupToolchainDetailsByKey[key]
    }

    func isRustupToolchainDetailLoading(for package: PackageItem) -> Bool {
        guard let key = rustupToolchainDetailKey(for: package) else { return false }
        return rustupToolchainDetailLoadingKeys.contains(key)
    }

    func isRustupToolchainDetailUnavailable(for package: PackageItem) -> Bool {
        guard let key = rustupToolchainDetailKey(for: package) else { return false }
        return rustupToolchainDetailUnavailableKeys.contains(key)
    }

    func ensureRustupToolchainDetail(for package: PackageItem, force: Bool = false) {
        guard let key = rustupToolchainDetailKey(for: package) else { return }
        guard force
            || (
                rustupToolchainDetailsByKey[key] == nil
                    && !rustupToolchainDetailLoadingKeys.contains(key)
            ) else {
            return
        }
        guard let service = service() else {
            rustupToolchainDetailLoadingKeys.remove(key)
            rustupToolchainDetailUnavailableKeys.insert(key)
            return
        }

        rustupToolchainDetailUnavailableKeys.remove(key)
        rustupToolchainDetailLoadingKeys.insert(key)
        withTimeout(
            30,
            source: "core.fetching",
            action: "getRustupToolchainDetail",
            managerId: "rustup",
            taskType: "refresh",
            operation: { completion in
                service.getRustupToolchainDetail(toolchain: package.name) { completion($0) }
            }
        ) { [weak self] jsonString in
            guard let self = self else { return }

            guard let jsonString,
                  let data = jsonString.data(using: .utf8),
                  let detail: CoreRustupToolchainDetail = self.decodeCorePayload(
                    CoreRustupToolchainDetail.self,
                    from: data,
                    decodeContext: "ensureRustupToolchainDetail",
                    source: "core.fetching",
                    action: "getRustupToolchainDetail.decode",
                    managerId: "rustup",
                    taskType: "refresh"
                  ) else {
                DispatchQueue.main.async {
                    self.rustupToolchainDetailLoadingKeys.remove(key)
                    self.rustupToolchainDetailUnavailableKeys.insert(key)
                }
                return
            }

            DispatchQueue.main.async {
                self.rustupToolchainDetailsByKey[key] = detail
                self.rustupToolchainDetailLoadingKeys.remove(key)
                self.rustupToolchainDetailUnavailableKeys.remove(key)
            }
        }
    }

    private func rustupToolchainDetailKey(for package: PackageItem) -> String? {
        guard package.managerId == "rustup", package.status != .available else { return nil }
        let normalizedName = package.name
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()
        guard !normalizedName.isEmpty else { return nil }
        return "rustup|\(normalizedName)"
    }

    private func stablePackageId(
        managerId: String,
        packageName: String,
        packageIdentifier: String? = nil
    ) -> String {
        let trimmedIdentifier = packageIdentifier?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        if !trimmedIdentifier.isEmpty {
            return "\(managerId):\(packageName)#\(trimmedIdentifier)"
        }
        return "\(managerId):\(packageName)"
    }

    private func packageIdentityId(
        managerId: String,
        packageName: String,
        packageIdentifier: String? = nil,
        version: String?
    ) -> String {
        let identityKey = PackageIdentity.normalizedIdentityKey(name: packageName, version: version)
        guard !identityKey.isEmpty else {
            return stablePackageId(
                managerId: managerId,
                packageName: packageName,
                packageIdentifier: packageIdentifier
            )
        }
        return "\(managerId):\(identityKey)"
    }

    private func availablePackageId(
        managerId: String,
        packageName: String,
        packageIdentifier: String? = nil,
        version: String?
    ) -> String {
        let stableId = stablePackageId(
            managerId: managerId,
            packageName: packageName,
            packageIdentifier: packageIdentifier
        )
        guard let version else { return stableId }
        let normalizedVersion = version.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !normalizedVersion.isEmpty else { return stableId }
        return "\(stableId)::\(normalizedVersion)"
    }

    // MARK: - Manager Status

    func fetchManagerStatus() {
        guard let svc = service() else { return }
        withTimeout(
            30,
            source: "core.fetching",
            action: "listManagerStatus",
            taskType: "refresh",
            operation: { completion in
            svc.listManagerStatus { completion($0) }
        }) { [weak self] jsonString in
            guard let self = self,
                  let jsonString = jsonString,
                  let data = jsonString.data(using: .utf8),
                  let statuses: [ManagerStatus] = self.decodeCorePayload(
                    [ManagerStatus].self,
                    from: data,
                    decodeContext: "fetchManagerStatus",
                    source: "core.fetching",
                    action: "listManagerStatus.decode",
                    taskType: "refresh",
                    keyDecodingStrategy: .useDefaultKeys
                  ) else { return }

            DispatchQueue.main.async {
                var map: [String: ManagerStatus] = [:]
                for status in statuses {
                    map[status.managerId] = status
                }
                self.managerStatuses = map
                self.detectedManagers = Set(
                    map.compactMap { entry in
                        entry.value.detected ? entry.key : nil
                    }
                )
                self.pruneOnboardingDetectionForDisabledManagers()
            }
        }
    }

    func fetchTaskOutput(taskId: String, completion: @escaping (CoreTaskOutputRecord?) -> Void) {
        guard let numericTaskId = Int64(taskId) else {
            completion(nil)
            return
        }
        guard let svc = service() else {
            recordLastError(
                source: "core.fetching",
                action: "getTaskOutput.service_unavailable",
                taskType: "diagnostics"
            )
            completion(nil)
            return
        }

        withTimeout(
            30,
            source: "core.fetching",
            action: "getTaskOutput",
            taskType: "diagnostics",
            operation: { callback in
            svc.getTaskOutput(taskId: numericTaskId) { callback($0) }
        }) { [weak self] jsonString in
            guard let self = self,
                  let jsonString = jsonString,
                  let data = jsonString.data(using: .utf8),
                  let output: CoreTaskOutputRecord = self.decodeCorePayload(
                    CoreTaskOutputRecord.self,
                    from: data,
                    decodeContext: "fetchTaskOutput",
                    source: "core.fetching",
                    action: "getTaskOutput.decode",
                    taskType: "diagnostics"
                  ) else {
                completion(nil)
                return
            }
            completion(output)
        }
    }

    func fetchTaskLogs(taskId: String, limit: Int, completion: @escaping ([CoreTaskLogRecord]?) -> Void) {
        guard let numericTaskId = Int64(taskId) else {
            completion(nil)
            return
        }
        guard let svc = service() else {
            recordLastError(
                source: "core.fetching",
                action: "listTaskLogs.service_unavailable",
                taskType: "diagnostics"
            )
            completion(nil)
            return
        }

        let clampedLimit = max(limit, 0)
        if clampedLimit == 0 {
            completion([])
            return
        }

        withTimeout(
            30,
            source: "core.fetching",
            action: "listTaskLogs",
            taskType: "diagnostics",
            operation: { callback in
            svc.listTaskLogs(taskId: numericTaskId, limit: Int64(clampedLimit)) { callback($0) }
        }) { [weak self] jsonString in
            guard let self = self,
                  let jsonString = jsonString,
                  let data = jsonString.data(using: .utf8),
                  let logs: [CoreTaskLogRecord] = self.decodeCorePayload(
                    [CoreTaskLogRecord].self,
                    from: data,
                    decodeContext: "fetchTaskLogs",
                    source: "core.fetching",
                    action: "listTaskLogs.decode",
                    taskType: "diagnostics"
                  ) else {
                completion(nil)
                return
            }
            completion(logs)
        }
    }

    func fetchTaskTimeoutPrompts() {
        guard let svc = service() else { return }
        withTimeout(
            10,
            source: "core.fetching",
            action: "listTaskTimeoutPrompts",
            taskType: "diagnostics",
            operation: { completion in
                svc.listTaskTimeoutPrompts { completion($0) }
            }
        ) { [weak self] jsonString in
            guard let self = self else { return }
            guard let jsonString = jsonString,
                  let data = jsonString.data(using: .utf8),
                  let prompts: [CoreTaskTimeoutPrompt] = self.decodeCorePayload(
                    [CoreTaskTimeoutPrompt].self,
                    from: data,
                    decodeContext: "fetchTaskTimeoutPrompts",
                    source: "core.fetching",
                    action: "listTaskTimeoutPrompts.decode",
                    taskType: "diagnostics"
                  ) else {
                DispatchQueue.main.async {
                    self.taskTimeoutPrompts = []
                }
                return
            }

            DispatchQueue.main.async {
                self.taskTimeoutPrompts = prompts
            }
        }
    }

}
