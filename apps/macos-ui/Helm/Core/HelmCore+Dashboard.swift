import Foundation
import os.log

private let taskSyncLogger = Logger(subsystem: "com.jasoncavinder.Helm", category: "core.tasks")

struct PackageDescriptionLookupCandidate {
    let managerId: String
    let packageName: String
    let version: String?
    let lookupKey: String
}

extension HelmCore {
    static let managerActionTaskMissingGraceSeconds: TimeInterval = 12
    static let packageDescriptionLookupTaskStaleSeconds: TimeInterval = 20
    static let localManagerActionTaskIdPrefix = "local-manager-action-"
    static let localManagerActionTaskRetentionSeconds: TimeInterval = 180
    static let managerVerificationTimeoutSeconds: TimeInterval = 120

    var allKnownPackages: [PackageItem] {
        if let cached = cachedAllKnownPackagesSorted {
            return cached
        }

        let sorted = sortedPackagesByDisplayName(allKnownPackagesUnsorted)
        cachedAllKnownPackagesSorted = sorted
        if cachedKnownPackageById.isEmpty {
            cachedKnownPackageById = Dictionary(uniqueKeysWithValues: sorted.map { ($0.id, $0) })
        }
        return sorted
    }

    private var allKnownPackagesUnsorted: [PackageItem] {
        if let cached = cachedAllKnownPackagesUnsorted {
            return cached
        }

        let enabledOutdated = outdatedPackages.filter { isManagerEnabled($0.managerId) }
        let outdatedStableIds = Set(
            enabledOutdated.map { package in
                packageDescriptionLookupKey(
                    managerId: package.managerId,
                    packageName: package.name,
                    version: nil
                )
            }
        )
        let installedOnly = installedPackages.filter {
            isManagerEnabled($0.managerId)
                && !outdatedStableIds.contains(
                    packageDescriptionLookupKey(
                        managerId: $0.managerId,
                        packageName: $0.name,
                        version: nil
                    )
                )
        }
        var combined = enabledOutdated + installedOnly

        let cachedById = cachedAvailablePackages
            .filter { isManagerEnabled($0.managerId) }
            .reduce(into: [String: PackageItem]()) { partial, package in
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

        cachedAllKnownPackagesUnsorted = combined
        cachedKnownPackageById = Dictionary(uniqueKeysWithValues: combined.map { ($0.id, $0) })
        return combined
    }

    func knownPackage(withId packageId: String) -> PackageItem? {
        if let package = cachedKnownPackageById[packageId] {
            return package
        }
        _ = allKnownPackagesUnsorted
        return cachedKnownPackageById[packageId]
    }

    private func sortedPackagesByDisplayName(_ packages: [PackageItem]) -> [PackageItem] {
        let keyed = packages.enumerated().map { index, package in
            (
                index: index,
                package: package,
                sortKey: package.displayName.folding(
                    options: [.caseInsensitive, .diacriticInsensitive],
                    locale: .current
                )
            )
        }
        return keyed
            .sorted { lhs, rhs in
                if lhs.sortKey == rhs.sortKey {
                    return lhs.index < rhs.index
                }
                return lhs.sortKey < rhs.sortKey
            }
            .map(\.package)
    }

    var visibleManagers: [ManagerInfo] {
        overviewState.visibleManagers
    }

    var failedTaskCount: Int {
        overviewState.failedTaskCount
    }

    var runningTaskCount: Int {
        overviewState.runningTaskCount
    }

    var aggregateHealth: OperationalHealth {
        overviewState.aggregateHealth
    }

    /// Returns a filtered package list grouped by package identity.
    /// Identity is package name plus optional variant qualifier derived from version selector.
    /// Merges local matches with remote search results, then applies manager/status filters.
    func filteredPackages(
        query: String,
        managerId: String?,
        statusFilter: PackageStatus?,
        pinnedOnly: Bool = false,
        knownPackages: [PackageItem]? = nil
    ) -> [ConsolidatedPackageItem] {
        let sourcePackages = knownPackages ?? allKnownPackages
        let consolidated = consolidatePackages(
            filteredPackagesRaw(
                query: query,
                managerId: managerId,
                statusFilter: statusFilter,
                pinnedOnly: pinnedOnly,
                knownPackages: sourcePackages
            )
        )
        return sortConsolidatedPackagesForQuery(consolidated, query: query)
    }

    /// Returns manager-scoped package rows used as canonical action targets.
    private func filteredPackagesRaw(
        query: String,
        managerId: String?,
        statusFilter: PackageStatus?,
        pinnedOnly: Bool,
        knownPackages: [PackageItem]? = nil
    ) -> [PackageItem] {
        var base = knownPackages ?? allKnownPackages
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()

        if !trimmed.isEmpty {
            let localMatches = base.filter {
                packageManagerParticipatesInSearch($0.managerId)
                    && packageMatchesQuery($0, queryToken: trimmed)
            }
            var mergedById = Dictionary(uniqueKeysWithValues: localMatches.map { ($0.id, $0) })
            for remote in searchResults
            where isManagerEnabled(remote.managerId)
                && packageManagerParticipatesInSearch(remote.managerId)
                && packageMatchesQuery(remote, queryToken: trimmed) {
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

            base = sortedPackagesByDisplayName(Array(mergedById.values))
        }

        if let managerId {
            base = base.filter { $0.managerId == managerId }
        }

        if pinnedOnly {
            base = base.filter(\.pinned)
        }

        if let statusFilter {
            base = base.filter { package in
                if statusFilter == .upgradable, package.pinned {
                    return false
                }
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

    private func packageManagerParticipatesInSearch(_ managerId: String) -> Bool {
        managerStatuses[managerId]?.supportsRemoteSearch ?? true
    }

    private func packageMatchesQuery(_ package: PackageItem, queryToken: String) -> Bool {
        guard !queryToken.isEmpty else { return true }
        let normalizedQueryToken = PackageIdentity.normalizedExactQueryToken(queryToken)
        if package.normalizedIdentityKey.contains(queryToken)
            || (
                !normalizedQueryToken.isEmpty
                    && normalizedQueryToken != queryToken
                    && package.normalizedIdentityKey.contains(normalizedQueryToken)
            ) {
            return true
        }
        if package.manager.lowercased().contains(queryToken) {
            return true
        }
        return package.summary?.lowercased().contains(queryToken) ?? false
    }

    func outdatedCount(forManagerId managerId: String) -> Int {
        managersState.outdatedCountByManager[managerId, default: 0]
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
        if isManagerVerifying(managerId) {
            return .notInstalled
        }
        let packageStateIssuePresent = !(managerStatuses[managerId]?.packageStateIssues?.isEmpty ?? true)
        if !packageStateIssuePresent, let precomputed = overviewState.managerHealthById[managerId] {
            return precomputed
        }
        if let status = managerStatuses[managerId], status.detected == false {
            if packageStateIssuePresent {
                return .attention
            }
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
        if packageStateIssuePresent {
            return .attention
        }
        if managerStatuses[managerId]?.multiInstanceState == "attention_needed" {
            return .attention
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
            && !isManagerUninstalling(package.managerId)
    }

    func canInstallPackage(_ package: PackageItem, includeAlternates: Bool = true) -> Bool {
        if canInstallPackageDirect(package) {
            return true
        }
        guard includeAlternates, package.status == .available else {
            return false
        }
        return installCandidates(for: package).contains { canInstallPackage($0, includeAlternates: false) }
    }

    func installActionInFlightPackageNames(knownPackages: [PackageItem]? = nil) -> Set<String> {
        let packageNameById = Dictionary(
            uniqueKeysWithValues: (knownPackages ?? allKnownPackages).map {
                (
                    $0.id,
                    PackageActionTracking.normalizedPackageIdentityKey(
                        name: $0.name,
                        version: $0.version
                    )
                )
            }
        )
        return PackageActionTracking.inFlightInstallNames(
            installActionPackageIds: installActionPackageIds,
            packageNameById: packageNameById,
            trackedNamesByPackageId: installActionNormalizedNameByPackageId
        )
    }

    func isInstallActionInFlight(for package: PackageItem, knownPackages: [PackageItem]? = nil) -> Bool {
        let normalizedPackageIdentity = PackageActionTracking.normalizedPackageIdentityKey(
            name: package.name,
            version: package.version
        )
        guard !normalizedPackageIdentity.isEmpty else {
            return installActionPackageIds.contains(package.id)
        }
        return installActionInFlightPackageNames(knownPackages: knownPackages)
            .contains(normalizedPackageIdentity)
    }

    func installCandidates(for package: PackageItem) -> [PackageItem] {
        guard package.status == .available else { return [] }
        let normalizedIdentityKey = package.normalizedIdentityKey
        guard !normalizedIdentityKey.isEmpty else { return [] }

        var candidatesByManager: [String: PackageItem] = [:]
        for candidate in allKnownPackagesUnsorted where candidate.status == .available {
            guard candidate.normalizedIdentityKey == normalizedIdentityKey else { continue }

            if let existing = candidatesByManager[candidate.managerId] {
                candidatesByManager[candidate.managerId] = preferredInstallCandidate(existing, candidate)
            } else {
                candidatesByManager[candidate.managerId] = candidate
            }
        }

        if candidatesByManager[package.managerId] == nil {
            candidatesByManager[package.managerId] = package
        }

        return candidatesByManager.values.sorted(by: installCandidateOrdering)
    }

    func preferredInstallCandidate(for package: PackageItem) -> PackageItem? {
        let candidates = installCandidates(for: package)
        return candidates.first(where: { canInstallPackage($0, includeAlternates: false) })
            ?? candidates.first
    }

    func canUninstallPackage(_ package: PackageItem) -> Bool {
        package.status != .available
            && (managerStatuses[package.managerId]?.supportsPackageUninstall ?? false)
            && isManagerEnabled(package.managerId)
            && !isManagerUninstalling(package.managerId)
    }

    func canPinPackage(_ package: PackageItem) -> Bool {
        package.status != .available
            && isManagerEnabled(package.managerId)
            && !isManagerUninstalling(package.managerId)
    }

    func supportsRemoteSearch(managerId: String) -> Bool {
        managerStatuses[managerId]?.supportsRemoteSearch ?? false
    }

    func isManagerEnabled(_ managerId: String) -> Bool {
        managerStatuses[managerId]?.enabled ?? true
    }

    func isManagerUninstalling(_ managerId: String) -> Bool {
        if let taskId = managerActionTaskByManager[managerId] {
            let taskType = managerActionTaskTypes[taskId]?.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
            if taskType == "manager_uninstall" {
                return true
            }
        }
        return false
    }

    func isManagerVerifying(_ managerId: String) -> Bool {
        verifyingManagerIds.contains(managerId)
    }

    func isManagerDetected(_ managerId: String) -> Bool {
        if isManagerVerifying(managerId) {
            return false
        }
        if let status = managerStatuses[managerId] {
            return status.detected
        }
        return detectedManagers.contains(managerId)
    }

    func packageDescriptionLookupKey(
        managerId: String,
        packageName: String,
        version: String?
    ) -> String {
        let normalizedManagerId = managerId.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        let normalizedPackageName = packageName
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()
        let normalizedVersion = normalizedDescriptionVersionToken(version) ?? ""
        return "\(normalizedManagerId)|\(normalizedPackageName)|\(normalizedVersion)"
    }

    func packageDescriptionSummary(for package: PackageItem) -> String? {
        for candidate in packageDescriptionLookupCandidates(for: package) {
            if let summary = packageDescriptionSummaryByKey[candidate.lookupKey] {
                let trimmed = summary.trimmingCharacters(in: .whitespacesAndNewlines)
                if !trimmed.isEmpty {
                    return trimmed
                }
            }
            if candidate.version != nil {
                let unversionedKey = packageDescriptionLookupKey(
                    managerId: candidate.managerId,
                    packageName: candidate.packageName,
                    version: nil
                )
                if let summary = packageDescriptionSummaryByKey[unversionedKey] {
                    let trimmed = summary.trimmingCharacters(in: .whitespacesAndNewlines)
                    if !trimmed.isEmpty {
                        return trimmed
                    }
                }
            }
        }
        let fallback = package.summary?.trimmingCharacters(in: .whitespacesAndNewlines)
        if fallback?.isEmpty == false {
            return fallback
        }
        return nil
    }

    func hasPackageDescriptionSummary(packageId: String) -> Bool {
        let package = descriptionLookupPackageById[packageId]
            ?? knownPackage(withId: packageId)
        guard let package else { return false }
        return packageDescriptionSummary(for: package) != nil
    }

    func packageDescriptionLookupCandidates(for package: PackageItem) -> [PackageDescriptionLookupCandidate] {
        let normalizedIdentityKey = package.normalizedIdentityKey
        guard !normalizedIdentityKey.isEmpty else { return [] }

        var preferredByManager: [String: PackageItem] = [:]
        for candidate in allKnownPackagesUnsorted {
            guard candidate.normalizedIdentityKey == normalizedIdentityKey else { continue }
            if let existing = preferredByManager[candidate.managerId] {
                preferredByManager[candidate.managerId] = preferredDescriptionLookupPackage(existing, candidate)
            } else {
                preferredByManager[candidate.managerId] = candidate
            }
        }

        if preferredByManager[package.managerId] == nil {
            preferredByManager[package.managerId] = package
        }

        return preferredByManager.values
            .sorted { lhs, rhs in
                let lhsPriority = managerPriorityRank(for: lhs.managerId)
                let rhsPriority = managerPriorityRank(for: rhs.managerId)
                if lhsPriority != rhsPriority {
                    return lhsPriority < rhsPriority
                }
                return normalizedManagerName(lhs.managerId)
                    .localizedCaseInsensitiveCompare(normalizedManagerName(rhs.managerId)) == .orderedAscending
            }
            .map { candidate in
                let targetVersion = packageDescriptionTargetVersion(for: candidate)
                return PackageDescriptionLookupCandidate(
                    managerId: candidate.managerId,
                    packageName: candidate.name,
                    version: targetVersion,
                    lookupKey: packageDescriptionLookupKey(
                        managerId: candidate.managerId,
                        packageName: candidate.name,
                        version: targetVersion
                    )
                )
            }
    }

    func syncManagerOperations(from coreTasks: [CoreTaskRecord]) {
        let statusById = Dictionary(uniqueKeysWithValues: coreTasks.map { ($0.id, $0.status.lowercased()) })
        let inFlightStates = Set(["queued", "running"])
        let mutationTaskTypesRequiringDetectionResync = Set(["manager_install", "manager_uninstall"])
        var completedManagerInstalls: [String] = []
        var completedManagerUninstalls: [String] = []
        var verificationManagerIds = Set<String>()

        for managerId in Array(managerActionTaskByManager.keys) {
            guard let taskId = managerActionTaskByManager[managerId] else { continue }
            let trackedTaskType = managerActionTaskTypes[taskId]?
                .trimmingCharacters(in: .whitespacesAndNewlines)
                .lowercased()
            guard let status = statusById[taskId] else {
                let submittedAt = managerActionTaskSubmittedAt[taskId] ?? .distantPast
                if Date().timeIntervalSince(submittedAt) < Self.managerActionTaskMissingGraceSeconds {
                    continue
                }
                if let trackedTaskType,
                   mutationTaskTypesRequiringDetectionResync.contains(trackedTaskType) {
                    verificationManagerIds.insert(managerId)
                    managerOperations[managerId] = L10n.App.Managers.Operation.verifying.localized
                }
                if let trackedTaskType,
                   mutationTaskTypesRequiringDetectionResync.contains(trackedTaskType) {
                    taskSyncLogger.warning(
                        "Tracked manager mutation task disappeared before visibility; scheduling manager verification (manager=\(managerId), task_id=\(taskId), task_type=\(trackedTaskType))"
                    )
                } else if let fallback = managerActionMissingTaskFailureText(for: trackedTaskType) {
                    managerOperations[managerId] = fallback
                    taskSyncLogger.warning(
                        "Tracked manager action task disappeared before visibility (manager=\(managerId), task_id=\(taskId), task_type=\(trackedTaskType ?? "unknown"))"
                    )
                } else {
                    managerOperations.removeValue(forKey: managerId)
                }
                managerActionTaskByManager.removeValue(forKey: managerId)
                managerActionTaskDescriptions.removeValue(forKey: taskId)
                managerActionTaskTypes.removeValue(forKey: taskId)
                managerActionTaskSubmittedAt.removeValue(forKey: taskId)
                continue
            }
            if !inFlightStates.contains(status) {
                if trackedTaskType == "manager_install" && status == "completed" {
                    completedManagerInstalls.append(managerId)
                } else if trackedTaskType == "manager_uninstall" && status == "completed" {
                    completedManagerUninstalls.append(managerId)
                } else if let trackedTaskType,
                          mutationTaskTypesRequiringDetectionResync.contains(trackedTaskType) {
                    verificationManagerIds.insert(managerId)
                }
                if trackedTaskType != "manager_install" || status != "completed" {
                    managerOperations.removeValue(forKey: managerId)
                }
                managerActionTaskByManager.removeValue(forKey: managerId)
                managerActionTaskDescriptions.removeValue(forKey: taskId)
                managerActionTaskTypes.removeValue(forKey: taskId)
                managerActionTaskSubmittedAt.removeValue(forKey: taskId)
            }
        }

        for managerId in completedManagerUninstalls {
            reconcileManagerAfterSuccessfulUninstall(managerId: managerId)
            startManagerVerification(managerId: managerId, coreTasks: coreTasks)
        }

        for managerId in completedManagerInstalls {
            startManagerVerification(managerId: managerId, coreTasks: coreTasks)
        }

        let completedVerificationManagers = Set(completedManagerInstalls).union(completedManagerUninstalls)
        for managerId in verificationManagerIds where !completedVerificationManagers.contains(managerId) {
            startManagerVerification(managerId: managerId, coreTasks: coreTasks)
        }

        syncManagerVerificationState(from: coreTasks)
    }

    private func startManagerVerification(managerId: String, coreTasks: [CoreTaskRecord]) {
        let latestDetectionTaskId = coreTasks
            .filter { task in
                task.manager == managerId && task.taskType.lowercased() == "detection"
            }
            .map(\.id)
            .max() ?? lastObservedTaskId

        verifyingManagerIds.insert(managerId)
        managerOperations[managerId] = L10n.App.Managers.Operation.verifying.localized
        managerVerificationAnchorTaskIdByManager[managerId] = latestDetectionTaskId
        managerVerificationStartedAtByManager[managerId] = Date()

        triggerDetection(for: managerId) { [weak self] success in
            guard let self else { return }
            guard self.verifyingManagerIds.contains(managerId) else { return }
            if success {
                return
            }

            taskSyncLogger.warning(
                "Manager-scoped verification detection trigger failed; falling back to full detection (manager=\(managerId, privacy: .public))"
            )
            self.finishManagerVerification(managerId: managerId, refreshSnapshots: false)
            self.triggerDetection()
        }
    }

    private func syncManagerVerificationState(from coreTasks: [CoreTaskRecord]) {
        guard !verifyingManagerIds.isEmpty else { return }

        let inFlightStatuses = Set(["queued", "running"])
        let terminalStatuses = Set(["completed", "failed", "cancelled"])
        let now = Date()

        for managerId in Array(verifyingManagerIds) {
            let anchorTaskId = managerVerificationAnchorTaskIdByManager[managerId] ?? 0
            let latestManagerDetectionTask = coreTasks
                .filter { task in
                    task.manager == managerId
                        && task.taskType.lowercased() == "detection"
                        && task.id > anchorTaskId
                }
                .max { lhs, rhs in
                    lhs.id < rhs.id
                }

            guard let latestManagerDetectionTask else {
                let startedAt = managerVerificationStartedAtByManager[managerId] ?? .distantPast
                if now.timeIntervalSince(startedAt) > Self.managerVerificationTimeoutSeconds {
                    taskSyncLogger.warning(
                        "Manager verification timed out waiting for manager-scoped detection task; falling back to full detection (manager=\(managerId, privacy: .public))"
                    )
                    finishManagerVerification(managerId: managerId, refreshSnapshots: false)
                    triggerDetection()
                }
                continue
            }

            let detectionStatus = latestManagerDetectionTask.status.lowercased()
            if inFlightStatuses.contains(detectionStatus) {
                continue
            }
            if terminalStatuses.contains(detectionStatus) {
                finishManagerVerification(managerId: managerId, refreshSnapshots: true)
            }
        }
    }

    private func finishManagerVerification(managerId: String, refreshSnapshots: Bool) {
        verifyingManagerIds.remove(managerId)
        managerOperations.removeValue(forKey: managerId)
        managerVerificationAnchorTaskIdByManager.removeValue(forKey: managerId)
        managerVerificationStartedAtByManager.removeValue(forKey: managerId)

        guard refreshSnapshots else { return }

        fetchManagerStatus()
        fetchPackages()
        fetchOutdatedPackages()
        refreshCachedAvailablePackages()
    }

    private func managerActionMissingTaskFailureText(for taskType: String?) -> String? {
        switch taskType {
        case "manager_install":
            return L10n.App.Managers.Operation.installFailed.localized
        case "manager_update":
            return L10n.App.Managers.Operation.updateFailed.localized
        case "manager_uninstall":
            return L10n.App.Managers.Operation.uninstallFailed.localized
        default:
            return nil
        }
    }

    private func reconcileManagerAfterSuccessfulUninstall(managerId: String) {
        // Keep multi-install managers in place until detection confirms instance state.
        if expectedRemainingInstallInstancesAfterUninstall(for: managerId) > 0 {
            return
        }

        installedPackages.removeAll { $0.managerId == managerId }
        outdatedPackages.removeAll { $0.managerId == managerId }
        searchResults.removeAll { $0.managerId == managerId }
        cachedAvailablePackages.removeAll { $0.managerId == managerId }

        if selectedManagerFilter == managerId {
            selectedManagerFilter = nil
        }

        detectedManagers.remove(managerId)
        managerStatuses.removeValue(forKey: managerId)
    }

    private func expectedRemainingInstallInstancesAfterUninstall(for managerId: String) -> Int {
        guard let status = managerStatuses[managerId] else {
            return 0
        }

        if let count = status.installInstanceCount {
            return max(count - 1, 0)
        }

        if let instances = status.installInstances, !instances.isEmpty {
            return max(instances.count - 1, 0)
        }

        return 0
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
                installActionNormalizedNameByPackageId.removeValue(forKey: packageId)
                continue
            }
            if inFlightStates.contains(status) {
                continue
            }

            installActionTaskByPackage.removeValue(forKey: packageId)
            installActionPackageIds.remove(packageId)
            installActionNormalizedNameByPackageId.removeValue(forKey: packageId)
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

    func syncRustupToolchainActions(from coreTasks: [CoreTaskRecord]) {
        let statusById = Dictionary(uniqueKeysWithValues: coreTasks.map { ($0.id, $0.status.lowercased()) })
        let inFlightStates = Set(["queued", "running"])
        let now = Date()
        var shouldRefreshSnapshots = false
        var packagesNeedingDetailRefresh: [PackageItem] = []

        for actionKey in Array(rustupToolchainActionTaskByKey.keys) {
            guard let taskId = rustupToolchainActionTaskByKey[actionKey] else { continue }
            guard let status = statusById[taskId] else {
                let submittedAt = rustupToolchainActionSubmittedAtByKey[actionKey]
                if taskId > lastObservedTaskId
                    || submittedAt.map({ now.timeIntervalSince($0) < Self.managerActionTaskMissingGraceSeconds }) == true
                {
                    continue
                }
                clearRustupToolchainActionTracking(for: actionKey)
                continue
            }

            if inFlightStates.contains(status) {
                continue
            }

            if let package = rustupToolchainActionPackageByKey[actionKey] {
                packagesNeedingDetailRefresh.append(package)
            }
            clearRustupToolchainActionTracking(for: actionKey)
            if status == "completed" {
                shouldRefreshSnapshots = true
            }
        }

        if shouldRefreshSnapshots {
            fetchPackages()
            fetchOutdatedPackages()
        }

        let uniquePackages = Dictionary(
            packagesNeedingDetailRefresh.map { ($0.id, $0) },
            uniquingKeysWith: { first, _ in first }
        ).values
        for package in uniquePackages {
            ensureRustupToolchainDetail(for: package, force: true)
        }
    }

    func syncUpgradePlanProjection(from coreTasks: [CoreTaskRecord]) {
        let stepIds = Set(upgradePlanSteps.map(\.id))
        guard !stepIds.isEmpty else {
            if !upgradePlanTaskProjectionByStepId.isEmpty {
                upgradePlanTaskProjectionByStepId = [:]
            }
            if !upgradePlanFailureGroups.isEmpty {
                upgradePlanFailureGroups = []
            }
            return
        }

        var latestByStepId: [String: CoreTaskRecord] = [:]
        for task in coreTasks where task.taskType.lowercased() == "upgrade" {
            guard let stepId = upgradePlanStepId(for: task), stepIds.contains(stepId) else { continue }
            if latestByStepId[stepId] == nil {
                latestByStepId[stepId] = task
            }
        }

        var projection = upgradePlanTaskProjectionByStepId.filter { stepIds.contains($0.key) }
        for (stepId, task) in latestByStepId {
            projection[stepId] = UpgradePlanTaskProjection(
                stepId: stepId,
                taskId: task.id,
                status: task.status.lowercased(),
                managerId: task.manager,
                labelKey: task.labelKey
            )
        }

        for (stepId, state) in projection where latestByStepId[stepId] == nil {
            let status = state.status.lowercased()
            if status == "queued" || status == "running" {
                // Preserve in-flight projections for tasks that have not been observed yet
                // in a listTasks snapshot.
                if state.taskId > lastObservedTaskId {
                    continue
                }
                projection.removeValue(forKey: stepId)
            }
        }

        upgradePlanTaskProjectionByStepId = projection
        rebuildUpgradePlanFailureGroups()
    }

    func rebuildUpgradePlanFailureGroups() {
        upgradePlanFailureGroups = buildUpgradePlanFailureGroups(from: upgradePlanTaskProjectionByStepId)
    }

    func syncPackageDescriptionLookups(from coreTasks: [CoreTaskRecord]) {
        let statusById = Dictionary(uniqueKeysWithValues: coreTasks.map { ($0.id, $0.status.lowercased()) })
        let inFlightStates = Set(["queued", "running"])
        let now = Date()

        for packageId in Array(descriptionLookupTaskIdsByPackage.keys) {
            if hasPackageDescriptionSummary(packageId: packageId) {
                packageDescriptionUnavailableIds.remove(packageId)
                packageDescriptionLoadingIds.remove(packageId)
                descriptionLookupTaskIdsByPackage.removeValue(forKey: packageId)
                descriptionLookupStartedAtByPackage.removeValue(forKey: packageId)
                descriptionLookupPackageById.removeValue(forKey: packageId)
                continue
            }

            guard let taskIds = descriptionLookupTaskIdsByPackage[packageId], !taskIds.isEmpty else {
                descriptionLookupTaskIdsByPackage.removeValue(forKey: packageId)
                descriptionLookupStartedAtByPackage.removeValue(forKey: packageId)
                packageDescriptionLoadingIds.remove(packageId)
                continue
            }

            let inFlightTaskIds = taskIds.filter { taskId in
                guard let status = statusById[taskId] else { return false }
                return inFlightStates.contains(status)
            }

            if !inFlightTaskIds.isEmpty {
                let startedAt = descriptionLookupStartedAtByPackage[packageId] ?? now
                if now.timeIntervalSince(startedAt) < Self.packageDescriptionLookupTaskStaleSeconds {
                    descriptionLookupTaskIdsByPackage[packageId] = inFlightTaskIds
                    packageDescriptionLoadingIds.insert(packageId)
                    continue
                }

                let sortedTaskIds = inFlightTaskIds.sorted()
                    .map(String.init)
                    .joined(separator: ",")
                taskSyncLogger.warning(
                    "Description lookup tasks exceeded stale threshold; forcing local cache reconciliation (package_id=\(packageId), task_ids=\(sortedTaskIds), age_seconds=\(Int(now.timeIntervalSince(startedAt))))"
                )
                descriptionLookupTaskIdsByPackage[packageId] = inFlightTaskIds
                descriptionLookupStartedAtByPackage[packageId] = now
                packageDescriptionLoadingIds.insert(packageId)
                guard let package = descriptionLookupPackageById[packageId] else {
                    continue
                }
                refreshPackageDescriptionSummaryFromLocalCache(for: package, clearTracking: false)
                continue
            }

            descriptionLookupTaskIdsByPackage.removeValue(forKey: packageId)
            descriptionLookupStartedAtByPackage.removeValue(forKey: packageId)
            packageDescriptionLoadingIds.remove(packageId)

            guard let package = descriptionLookupPackageById[packageId] else {
                packageDescriptionUnavailableIds.insert(packageId)
                continue
            }

            packageDescriptionLoadingIds.insert(packageId)
            refreshPackageDescriptionSummaryFromLocalCache(for: package)
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

    func latestDetectionTask(for managerId: String) -> CoreTaskRecord? {
        latestCoreTasksSnapshot
            .filter { task in
                task.manager == managerId && task.taskType.lowercased() == "detection"
            }
            .max { lhs, rhs in lhs.id < rhs.id }
    }

    func managerDetectionDiagnostics(for managerId: String) -> ManagerDetectionDiagnostics {
        let managerInfo = ManagerInfo.find(byId: managerId)
        let status = managerStatuses[managerId]
        let isImplemented = status?.isImplemented ?? managerInfo?.isImplemented ?? true
        let isEnabled = status?.enabled ?? true
        let isDetected = isManagerDetected(managerId)
        let hasPackageStateIssues = !(status?.packageStateIssues?.isEmpty ?? true)
        let latestTask = latestDetectionTask(for: managerId)
        let latestStatus = latestTask?.status.lowercased()

        let reason: ManagerDetectionDiagnosticReason
        if !isImplemented {
            reason = .notImplemented
        } else if !isEnabled {
            reason = .disabled
        } else if hasPackageStateIssues {
            reason = .inconsistent
        } else if latestStatus == "queued" || latestStatus == "running" {
            reason = .inProgress
        } else if latestStatus == "failed" || latestStatus == "cancelled" {
            reason = .failed
        } else if isDetected {
            reason = .detected
        } else if latestStatus == "completed" {
            reason = .notDetected
        } else {
            reason = .neverChecked
        }

        return ManagerDetectionDiagnostics(
            reason: reason,
            latestTaskId: latestTask?.id,
            latestTaskStatus: latestStatus
        )
    }

    private func mergeSummary(into package: inout PackageItem, from candidate: String?) {
        let existingSummary = package.summary?.trimmingCharacters(in: .whitespacesAndNewlines)
        guard existingSummary?.isEmpty != false else { return }
        guard let candidate else { return }
        let trimmedCandidate = candidate.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedCandidate.isEmpty else { return }
        package.summary = trimmedCandidate
    }

    private func preferredDescriptionLookupPackage(_ lhs: PackageItem, _ rhs: PackageItem) -> PackageItem {
        let lhsUpgradable = lhs.status == .upgradable && !lhs.pinned
        let rhsUpgradable = rhs.status == .upgradable && !rhs.pinned
        if lhsUpgradable != rhsUpgradable {
            return lhsUpgradable ? lhs : rhs
        }

        let lhsAvailable = lhs.status == .available
        let rhsAvailable = rhs.status == .available
        if lhsAvailable != rhsAvailable {
            return lhsAvailable ? lhs : rhs
        }

        let lhsVersion = normalizedDescriptionVersionToken(lhs.version)
        let rhsVersion = normalizedDescriptionVersionToken(rhs.version)
        if lhsVersion == nil, rhsVersion != nil {
            return rhs
        }
        if lhsVersion != nil, rhsVersion == nil {
            return lhs
        }
        if let lhsVersion, let rhsVersion {
            let order = lhsVersion.compare(rhsVersion, options: [.numeric, .caseInsensitive])
            if order != .orderedSame {
                return order == .orderedDescending ? lhs : rhs
            }
        }

        let lhsSummary = lhs.summary?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        let rhsSummary = rhs.summary?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        if lhsSummary.isEmpty != rhsSummary.isEmpty {
            return lhsSummary.isEmpty ? rhs : lhs
        }
        return lhs
    }

    private func packageDescriptionTargetVersion(for package: PackageItem) -> String? {
        if package.status == .upgradable && !package.pinned {
            let latest = normalizedDescriptionVersionToken(package.latestVersion)
            if let latest {
                return latest
            }
        }
        return normalizedDescriptionVersionToken(package.version)
    }

    private func normalizedDescriptionVersionToken(_ value: String?) -> String? {
        guard let value else { return nil }
        let normalized = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !normalized.isEmpty else { return nil }
        let unknownLabel = L10n.Common.unknown.localized.lowercased()
        if normalized.lowercased() == unknownLabel {
            return nil
        }
        return normalized
    }

    private func canInstallPackageDirect(_ package: PackageItem) -> Bool {
        package.status == .available
            && (managerStatuses[package.managerId]?.supportsPackageInstall ?? false)
            && isManagerEnabled(package.managerId)
            && !isManagerUninstalling(package.managerId)
    }

    private func preferredInstallCandidate(_ lhs: PackageItem, _ rhs: PackageItem) -> PackageItem {
        let lhsVersion = normalizedDescriptionVersionToken(lhs.version)
        let rhsVersion = normalizedDescriptionVersionToken(rhs.version)
        if lhsVersion == nil, rhsVersion != nil {
            return rhs
        }
        if lhsVersion != nil, rhsVersion == nil {
            return lhs
        }
        if let lhsVersion, let rhsVersion {
            let order = lhsVersion.compare(rhsVersion, options: [.numeric, .caseInsensitive])
            if order != .orderedSame {
                return order == .orderedDescending ? lhs : rhs
            }
        }

        let lhsSummary = lhs.summary?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        let rhsSummary = rhs.summary?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        if lhsSummary.isEmpty != rhsSummary.isEmpty {
            return lhsSummary.isEmpty ? rhs : lhs
        }
        return lhs
    }

    private func installCandidateOrdering(_ lhs: PackageItem, _ rhs: PackageItem) -> Bool {
        let lhsInstallable = canInstallPackage(lhs, includeAlternates: false)
        let rhsInstallable = canInstallPackage(rhs, includeAlternates: false)
        if lhsInstallable != rhsInstallable {
            return lhsInstallable && !rhsInstallable
        }

        let lhsPriority = managerPriorityRank(for: lhs.managerId)
        let rhsPriority = managerPriorityRank(for: rhs.managerId)
        if lhsPriority != rhsPriority {
            return lhsPriority < rhsPriority
        }

        return normalizedManagerName(lhs.managerId)
            .localizedCaseInsensitiveCompare(normalizedManagerName(rhs.managerId)) == .orderedAscending
    }

    private func sortConsolidatedPackagesForQuery(
        _ packages: [ConsolidatedPackageItem],
        query: String
    ) -> [ConsolidatedPackageItem] {
        let queryQualifiedToken = PackageIdentity.normalizedExactQueryToken(query)
        let queryBaseToken = PackageIdentity.normalizedQueryBaseToken(query)
        guard !queryBaseToken.isEmpty else { return packages }
        let queryHasQualifier = queryQualifiedToken.contains("@")

        let indexedPackages = Array(packages.enumerated())
        return indexedPackages
            .sorted { lhs, rhs in
                let lhsRank = exactMatchRank(
                    for: lhs.element.package,
                    queryBaseToken: queryBaseToken,
                    queryQualifiedToken: queryQualifiedToken,
                    queryHasQualifier: queryHasQualifier
                )
                let rhsRank = exactMatchRank(
                    for: rhs.element.package,
                    queryBaseToken: queryBaseToken,
                    queryQualifiedToken: queryQualifiedToken,
                    queryHasQualifier: queryHasQualifier
                )
                if lhsRank != rhsRank {
                    return lhsRank < rhsRank
                }
                return lhs.offset < rhs.offset
            }
            .map(\.element)
    }

    private func exactMatchRank(
        for package: PackageItem,
        queryBaseToken: String,
        queryQualifiedToken: String,
        queryHasQualifier: Bool
    ) -> Int {
        if queryHasQualifier, package.normalizedIdentityKey == queryQualifiedToken {
            return 0
        }
        if package.normalizedBaseName == queryBaseToken {
            return 1
        }
        return 2
    }

    private func consolidatePackages(_ packages: [PackageItem]) -> [ConsolidatedPackageItem] {
        ConsolidatedPackageItem.consolidate(
            packages,
            localizedManagerName: { managerId in
                normalizedManagerName(managerId)
            }
        )
    }

    private func upgradePlanStepId(for task: CoreTaskRecord) -> String? {
        UpgradePreviewPlanner.planStepId(managerId: task.manager, labelArgs: task.labelArgs)
    }

    private func buildUpgradePlanFailureGroups(
        from projection: [String: UpgradePlanTaskProjection]
    ) -> [UpgradePlanFailureGroup] {
        var stepsById: [String: CoreUpgradePlanStep] = [:]
        for step in upgradePlanSteps where stepsById[step.id] == nil {
            stepsById[step.id] = step
        }
        var grouped: [String: (managerId: String, stepIds: Set<String>, packageNames: Set<String>)] = [:]

        for (stepId, state) in projection where state.status.lowercased() == "failed" {
            let managerId = stepsById[stepId]?.managerId ?? state.managerId
            let packageName = stepsById[stepId]?.packageName ?? stepId

            if var entry = grouped[managerId] {
                entry.stepIds.insert(stepId)
                entry.packageNames.insert(packageName)
                grouped[managerId] = entry
            } else {
                grouped[managerId] = (
                    managerId: managerId,
                    stepIds: [stepId],
                    packageNames: [packageName]
                )
            }
        }

        return grouped.values
            .map { entry in
                UpgradePlanFailureGroup(
                    id: entry.managerId,
                    managerId: entry.managerId,
                    stepIds: entry.stepIds.sorted(),
                    packageNames: entry.packageNames.sorted()
                )
            }
            .sorted { lhs, rhs in
                if lhs.stepIds.count == rhs.stepIds.count {
                    return lhs.managerId.localizedCaseInsensitiveCompare(rhs.managerId) == .orderedAscending
                }
                return lhs.stepIds.count > rhs.stepIds.count
            }
    }
}
