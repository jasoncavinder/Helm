import Foundation
import os.log

private let logger = Logger(subsystem: "com.jasoncavinder.Helm", category: "core.actions")

extension HelmCore {
    private static let scopedUpgradePlanPhaseTimeoutSeconds: TimeInterval = 300

    func cancelTask(_ task: TaskItem) {
        guard task.isRunning, let taskId = Int64(task.id) else { return }
        service()?.cancelTask(taskId: taskId) { [weak self] success in
            DispatchQueue.main.async {
                if success {
                    // Optimistically mark the task as cancelled before the next poll
                    if let idx = self?.activeTasks.firstIndex(where: { $0.id == task.id }) {
                        self?.activeTasks[idx] = TaskItem(
                            id: task.id,
                            description: task.description,
                            status: "Cancelled",
                            managerId: task.managerId,
                            taskType: task.taskType,
                            labelKey: task.labelKey,
                            labelArgs: task.labelArgs
                        )
                    }
                    self?.postAccessibilityAnnouncement(
                        L10n.Service.Task.Status.cancelled.localized
                    )
                } else {
                    logger.warning("cancelTask(\(taskId)) returned false")
                }
            }
        }
    }

    func upgradePackage(_ package: PackageItem) {
        guard canUpgradeIndividually(package), !upgradeActionPackageIds.contains(package.id) else { return }

        DispatchQueue.main.async {
            self.upgradeActionPackageIds.insert(package.id)
        }

        guard let service = service() else {
            logger.error("upgradePackage(\(package.managerId):\(package.name)) failed: service unavailable")
            DispatchQueue.main.async {
                self.upgradeActionPackageIds.remove(package.id)
            }
            return
        }

        withTimeout(300, operation: { completion in
            service.upgradePackage(managerId: package.managerId, packageName: package.name) { completion($0) }
        }, fallback: Int64(-1)) { [weak self] taskId in
            DispatchQueue.main.async {
                guard let self = self, let taskId = taskId else { return }
                if taskId < 0 {
                    self.upgradeActionTaskByPackage.removeValue(forKey: package.id)
                    self.upgradeActionPackageIds.remove(package.id)
                    logger.error("upgradePackage(\(package.managerId):\(package.name)) failed")
                    return
                }

                self.upgradeActionTaskByPackage[package.id] = UInt64(taskId)
                self.registerManagerActionTask(
                    managerId: package.managerId,
                    taskId: UInt64(taskId),
                    description: self.upgradeActionDescription(for: package),
                    inProgressText: L10n.App.Managers.Operation.upgrading.localized
                )
            }
        }
    }

    func retryFailedUpgradePlanSteps() {
        let failedStepIds = upgradePlanSteps
            .filter { projectedUpgradePlanStatus(for: $0).lowercased() == "failed" }
            .map(\.id)
        retryUpgradePlanSteps(stepIds: failedStepIds)
    }

    func retryUpgradePlanSteps(stepIds: [String]) {
        guard !stepIds.isEmpty else { return }
        var stepsById: [String: CoreUpgradePlanStep] = [:]
        for step in upgradePlanSteps where stepsById[step.id] == nil {
            stepsById[step.id] = step
        }

        for stepId in stepIds {
            guard let step = stepsById[stepId] else { continue }
            guard projectedUpgradePlanStatus(for: step).lowercased() == "failed" else { continue }
            retryUpgradePlanStep(step)
        }
    }

    func runUpgradePlanScoped(managerScopeId: String, packageFilter: String) {
        let scopedSteps = HelmCore.scopedUpgradePlanSteps(
            from: upgradePlanSteps,
            managerScopeId: managerScopeId,
            packageFilter: packageFilter
        )
        let runCandidateSteps = scopedSteps.filter { step in
            let status = projectedUpgradePlanStatus(for: step)
            let hasProjectedTask = upgradePlanTaskProjectionByStepId[step.id] != nil
            return UpgradePreviewPlanner.shouldRunScopedStep(
                status: status,
                hasProjectedTask: hasProjectedTask,
                managerId: step.managerId,
                safeModeEnabled: safeModeEnabled
            )
        }

        guard !runCandidateSteps.isEmpty else { return }

        let runToken = UUID()
        scopedUpgradePlanRunToken = runToken
        scopedUpgradePlanRunInProgress = true

        let phasesByRank = Dictionary(grouping: runCandidateSteps) { step in
            HelmCore.authorityRank(for: step.authority)
        }
        let orderedPhaseRanks = phasesByRank.keys.sorted()
        guard !orderedPhaseRanks.isEmpty else {
            finishScopedUpgradePlanRun(runToken: runToken, invalidateToken: true)
            return
        }

        runScopedUpgradePlanPhases(
            phaseRanks: orderedPhaseRanks,
            phasesByRank: phasesByRank,
            phaseIndex: 0,
            runToken: runToken
        )
    }

    func cancelRemainingUpgradePlanSteps(managerScopeId: String, packageFilter: String) {
        scopedUpgradePlanRunToken = UUID()
        scopedUpgradePlanRunInProgress = false

        let scopedStepIds = Set(
            HelmCore.scopedUpgradePlanSteps(
                from: upgradePlanSteps,
                managerScopeId: managerScopeId,
                packageFilter: packageFilter
            )
            .map(\.id)
        )
        guard !scopedStepIds.isEmpty else { return }

        var cancelledTaskIds = Set<Int64>()
        for task in activeTasks where task.isRunning && task.taskType?.lowercased() == "upgrade" {
            guard let stepId = upgradePlanStepId(from: task), scopedStepIds.contains(stepId) else { continue }
            if let taskId = Int64(task.id) {
                cancelledTaskIds.insert(taskId)
            }
            cancelTask(task)
        }

        let plannerProjectionByStepId = upgradePlanTaskProjectionByStepId.reduce(
            into: [String: UpgradePreviewPlanner.ProjectedTaskState]()
        ) { partial, entry in
            partial[entry.key] = .init(
                taskId: entry.value.taskId,
                status: entry.value.status
            )
        }
        let projectedTaskIds = UpgradePreviewPlanner.projectedTaskIdsForCancellation(
            scopedStepIds: scopedStepIds,
            projections: plannerProjectionByStepId
        )
        let remainingTaskIds = projectedTaskIds.subtracting(cancelledTaskIds)

        for taskId in remainingTaskIds {
            service()?.cancelTask(taskId: taskId) { success in
                if !success {
                    logger.warning("cancelTask(\(taskId)) returned false")
                }
            }
        }
    }

    private func runScopedUpgradePlanPhases(
        phaseRanks: [Int],
        phasesByRank: [Int: [CoreUpgradePlanStep]],
        phaseIndex: Int,
        runToken: UUID
    ) {
        guard runToken == scopedUpgradePlanRunToken else {
            return
        }

        guard phaseIndex < phaseRanks.count else {
            finishScopedUpgradePlanRun(runToken: runToken, invalidateToken: true)
            return
        }

        let phaseRank = phaseRanks[phaseIndex]
        let phaseSteps = phasesByRank[phaseRank] ?? []
        guard !phaseSteps.isEmpty else {
            runScopedUpgradePlanPhases(
                phaseRanks: phaseRanks,
                phasesByRank: phasesByRank,
                phaseIndex: phaseIndex + 1,
                runToken: runToken
            )
            return
        }

        let dispatchGroup = DispatchGroup()
        for step in phaseSteps {
            dispatchGroup.enter()
            retryUpgradePlanStep(step) { _ in
                dispatchGroup.leave()
            }
        }

        dispatchGroup.notify(queue: .main) { [weak self] in
            self?.waitForScopedUpgradePlanPhaseCompletion(stepIds: Set(phaseSteps.map(\.id)), runToken: runToken) {
                self?.runScopedUpgradePlanPhases(
                    phaseRanks: phaseRanks,
                    phasesByRank: phasesByRank,
                    phaseIndex: phaseIndex + 1,
                    runToken: runToken
                )
            }
        }
    }

    private func waitForScopedUpgradePlanPhaseCompletion(
        stepIds: Set<String>,
        runToken: UUID,
        startedAt: Date = Date(),
        completion: @escaping () -> Void
    ) {
        guard !stepIds.isEmpty else {
            completion()
            return
        }

        DispatchQueue.main.asyncAfter(deadline: .now() + 0.35) { [weak self] in
            guard let self = self else { return }
            guard runToken == self.scopedUpgradePlanRunToken else {
                return
            }
            if Date().timeIntervalSince(startedAt) > Self.scopedUpgradePlanPhaseTimeoutSeconds {
                logger.warning(
                    "Scoped upgrade phase timed out after \(Self.scopedUpgradePlanPhaseTimeoutSeconds)s; cancelling scoped run token"
                )
                self.finishScopedUpgradePlanRun(runToken: runToken, invalidateToken: true)
                return
            }

            let inFlight = self.upgradePlanSteps.contains { step in
                guard stepIds.contains(step.id) else { return false }
                let status = self.projectedUpgradePlanStatus(for: step)
                let hasProjectedTask = self.upgradePlanTaskProjectionByStepId[step.id] != nil
                return UpgradePreviewPlanner.isInFlightStatus(
                    status: status,
                    hasProjectedTask: hasProjectedTask
                )
            }

            if inFlight {
                self.waitForScopedUpgradePlanPhaseCompletion(
                    stepIds: stepIds,
                    runToken: runToken,
                    startedAt: startedAt,
                    completion: completion
                )
            } else {
                completion()
            }
        }
    }

    private func finishScopedUpgradePlanRun(runToken: UUID, invalidateToken: Bool) {
        guard runToken == scopedUpgradePlanRunToken else { return }
        if invalidateToken {
            scopedUpgradePlanRunToken = UUID()
        }
        scopedUpgradePlanRunInProgress = false
    }

    private func retryUpgradePlanStep(_ step: CoreUpgradePlanStep, completion: ((Bool) -> Void)? = nil) {
        guard let service = service() else {
            logger.error("retryUpgradePlanStep(\(step.id)) failed: service unavailable")
            completion?(false)
            return
        }

        withTimeout(300, operation: { completion in
            service.upgradePackage(managerId: step.managerId, packageName: step.packageName) { completion($0) }
        }, fallback: Int64(-1)) { [weak self] taskId in
            DispatchQueue.main.async {
                guard let self = self, let taskId = taskId else {
                    completion?(false)
                    return
                }
                guard taskId >= 0 else {
                    logger.error("retryUpgradePlanStep(\(step.id)) failed: upgradePackage returned \(taskId)")
                    completion?(false)
                    return
                }
                self.upgradePlanTaskProjectionByStepId[step.id] = UpgradePlanTaskProjection(
                    stepId: step.id,
                    taskId: UInt64(taskId),
                    status: "queued",
                    managerId: step.managerId,
                    labelKey: step.reasonLabelKey
                )
                self.rebuildUpgradePlanFailureGroups()
                completion?(true)
            }
        }
    }

    private func upgradePlanStepId(from task: TaskItem) -> String? {
        UpgradePreviewPlanner.planStepId(managerId: task.managerId, labelArgs: task.labelArgs)
    }

    func installPackage(_ package: PackageItem) {
        guard canInstallPackage(package), !installActionPackageIds.contains(package.id) else { return }

        DispatchQueue.main.async {
            self.installActionPackageIds.insert(package.id)
        }

        guard let service = service() else {
            logger.error("installPackage(\(package.managerId):\(package.name)) failed: service unavailable")
            DispatchQueue.main.async {
                self.installActionPackageIds.remove(package.id)
            }
            return
        }

        withTimeout(300, operation: { completion in
            service.installPackage(managerId: package.managerId, packageName: package.name) { completion($0) }
        }, fallback: Int64(-1)) { [weak self] taskId in
            DispatchQueue.main.async {
                guard let self = self, let taskId = taskId else { return }
                if taskId < 0 {
                    self.installActionTaskByPackage.removeValue(forKey: package.id)
                    self.installActionPackageIds.remove(package.id)
                    logger.error("installPackage(\(package.managerId):\(package.name)) failed")
                    return
                }

                self.installActionTaskByPackage[package.id] = UInt64(taskId)
            }
        }
    }

    func uninstallPackage(_ package: PackageItem) {
        guard canUninstallPackage(package), !uninstallActionPackageIds.contains(package.id) else { return }

        DispatchQueue.main.async {
            self.uninstallActionPackageIds.insert(package.id)
        }

        guard let service = service() else {
            logger.error("uninstallPackage(\(package.managerId):\(package.name)) failed: service unavailable")
            DispatchQueue.main.async {
                self.uninstallActionPackageIds.remove(package.id)
            }
            return
        }

        withTimeout(300, operation: { completion in
            service.uninstallPackage(managerId: package.managerId, packageName: package.name) { completion($0) }
        }, fallback: Int64(-1)) { [weak self] taskId in
            DispatchQueue.main.async {
                guard let self = self, let taskId = taskId else { return }
                if taskId < 0 {
                    self.uninstallActionTaskByPackage.removeValue(forKey: package.id)
                    self.uninstallActionPackageIds.remove(package.id)
                    logger.error("uninstallPackage(\(package.managerId):\(package.name)) failed")
                    return
                }

                self.uninstallActionTaskByPackage[package.id] = UInt64(taskId)
            }
        }
    }

    func pinPackage(_ package: PackageItem) {
        DispatchQueue.main.async {
            self.pinActionPackageIds.insert(package.id)
        }
        guard let service = service() else {
            logger.error("pinPackage(\(package.managerId):\(package.name)) failed: service unavailable")
            DispatchQueue.main.async {
                self.pinActionPackageIds.remove(package.id)
            }
            return
        }
        let version = package.version.isEmpty || package.version == "unknown" ? nil : package.version
        service.pinPackage(managerId: package.managerId, packageName: package.name, version: version) { [weak self] success in
            DispatchQueue.main.async {
                self?.pinActionPackageIds.remove(package.id)
                if success {
                    self?.setPinnedState(packageId: package.id, pinned: true)
                    self?.fetchPackages()
                    self?.fetchOutdatedPackages()
                } else {
                    logger.error("pinPackage(\(package.managerId):\(package.name)) failed")
                }
            }
        }
    }

    func unpinPackage(_ package: PackageItem) {
        DispatchQueue.main.async {
            self.pinActionPackageIds.insert(package.id)
        }
        guard let service = service() else {
            logger.error("unpinPackage(\(package.managerId):\(package.name)) failed: service unavailable")
            DispatchQueue.main.async {
                self.pinActionPackageIds.remove(package.id)
            }
            return
        }
        service.unpinPackage(managerId: package.managerId, packageName: package.name) { [weak self] success in
            DispatchQueue.main.async {
                self?.pinActionPackageIds.remove(package.id)
                if success {
                    self?.setPinnedState(packageId: package.id, pinned: false)
                    self?.fetchPackages()
                    self?.fetchOutdatedPackages()
                } else {
                    logger.error("unpinPackage(\(package.managerId):\(package.name)) failed")
                }
            }
        }
    }

    func setManagerEnabled(_ managerId: String, enabled: Bool) {
        service()?.setManagerEnabled(managerId: managerId, enabled: enabled) { success in
            if !success {
                logger.error("setManagerEnabled(\(managerId), \(enabled)) failed")
            }
        }
    }

    func installManager(_ managerId: String) {
        DispatchQueue.main.async {
            self.managerOperations[managerId] = L10n.App.Managers.Operation.startingInstall.localized
        }
        guard let svc = service() else { return }
        withTimeout(300, operation: { completion in
            svc.installManager(managerId: managerId) { completion($0) }
        }, fallback: Int64(-1)) { [weak self] taskId in
            DispatchQueue.main.async {
                guard let self = self, let taskId = taskId else { return }
                if taskId < 0 {
                    logger.error("installManager(\(managerId)) failed")
                    self.consumeLastServiceErrorKey { serviceErrorKey in
                        self.managerOperations[managerId] =
                            serviceErrorKey?.localized ?? L10n.App.Managers.Operation.installFailed.localized
                    }
                    return
                }
                self.registerManagerActionTask(
                    managerId: managerId,
                    taskId: UInt64(taskId),
                    description: self.managerActionDescription(action: "Install", managerId: managerId),
                    inProgressText: L10n.App.Managers.Operation.installing.localized
                )
            }
        }
    }

    func updateManager(_ managerId: String) {
        DispatchQueue.main.async {
            self.managerOperations[managerId] = L10n.App.Managers.Operation.startingUpdate.localized
        }
        guard let svc = service() else { return }
        withTimeout(300, operation: { completion in
            svc.updateManager(managerId: managerId) { completion($0) }
        }, fallback: Int64(-1)) { [weak self] taskId in
            DispatchQueue.main.async {
                guard let self = self, let taskId = taskId else { return }
                if taskId < 0 {
                    logger.error("updateManager(\(managerId)) failed")
                    self.consumeLastServiceErrorKey { serviceErrorKey in
                        self.managerOperations[managerId] =
                            serviceErrorKey?.localized ?? L10n.App.Managers.Operation.updateFailed.localized
                    }
                    return
                }
                self.registerManagerActionTask(
                    managerId: managerId,
                    taskId: UInt64(taskId),
                    description: self.managerActionDescription(action: "Update", managerId: managerId),
                    inProgressText: L10n.App.Managers.Operation.updating.localized
                )
            }
        }
    }

    func uninstallManager(_ managerId: String) {
        DispatchQueue.main.async {
            self.managerOperations[managerId] = L10n.App.Managers.Operation.startingUninstall.localized
        }
        guard let svc = service() else { return }
        withTimeout(300, operation: { completion in
            svc.uninstallManager(managerId: managerId) { completion($0) }
        }, fallback: Int64(-1)) { [weak self] taskId in
            DispatchQueue.main.async {
                guard let self = self, let taskId = taskId else { return }
                if taskId < 0 {
                    logger.error("uninstallManager(\(managerId)) failed")
                    self.consumeLastServiceErrorKey { serviceErrorKey in
                        self.managerOperations[managerId] =
                            serviceErrorKey?.localized ?? L10n.App.Managers.Operation.uninstallFailed.localized
                    }
                    return
                }
                self.registerManagerActionTask(
                    managerId: managerId,
                    taskId: UInt64(taskId),
                    description: self.managerActionDescription(action: "Uninstall", managerId: managerId),
                    inProgressText: L10n.App.Managers.Operation.uninstalling.localized
                )
            }
        }
    }

    func registerManagerActionTask(
        managerId: String,
        taskId: UInt64,
        description: String,
        inProgressText: String
    ) {
        managerActionTaskDescriptions[taskId] = description
        managerActionTaskByManager[managerId] = taskId
        managerOperations[managerId] = inProgressText

        let idString = "\(taskId)"
        if !activeTasks.contains(where: { $0.id == idString }) {
            activeTasks.insert(
                TaskItem(
                    id: idString,
                    description: description,
                    status: "Queued",
                    managerId: managerId,
                    taskType: nil,
                    labelKey: nil,
                    labelArgs: nil
                ),
                at: 0
            )
        }
    }

    // MARK: - Search Orchestration

    func remoteSearchManagerIds() -> [String] {
        guard !managerStatuses.isEmpty else {
            if !detectedManagers.isEmpty {
                return ManagerInfo.all
                    .map(\.id)
                    .filter { supportsRemoteSearch(managerId: $0) }
                    .filter { detectedManagers.contains($0) }
            }
            return []
        }

        return ManagerInfo.all
            .map(\.id)
            .filter { supportsRemoteSearch(managerId: $0) }
            .filter { managerStatuses[$0]?.isImplemented ?? true }
            .filter { managerStatuses[$0]?.enabled ?? true }
            .filter { managerStatuses[$0]?.detected ?? true }
            .filter { detectedManagers.isEmpty || detectedManagers.contains($0) }
    }

    func onSearchTextChanged(_ query: String) {
        // 1. Instant local cache query
        fetchSearchResults(query: query)

        // 2. Cancel in-flight remote search
        cancelActiveRemoteSearch()

        // 3. Invalidate debounce timer
        searchDebounceTimer?.invalidate()
        searchDebounceTimer = nil

        // 4. If empty, clear state and return
        guard !query.trimmingCharacters(in: .whitespaces).isEmpty else {
            isSearching = false
            return
        }

        // 5. Start 300ms debounce timer for remote search
        searchDebounceTimer = Timer.scheduledTimer(withTimeInterval: 0.3, repeats: false) { [weak self] _ in
            self?.triggerRemoteSearch(query: query)
        }
    }

    func triggerRemoteSearch(query: String) {
        let managerIds = remoteSearchManagerIds()
        guard !managerIds.isEmpty else {
            isSearching = false
            return
        }

        isSearching = true
        for managerId in managerIds {
            service()?.triggerRemoteSearchForManager(managerId: managerId, query: query) { [weak self] taskId in
                DispatchQueue.main.async {
                    guard let self = self else { return }
                    if taskId >= 0 {
                        self.activeRemoteSearchTaskIds.insert(taskId)
                    } else {
                        logger.warning("triggerRemoteSearchForManager(\(managerId)) returned error")
                    }
                }
            }
        }
    }

    func triggerAvailablePackagesWarmupSearch() {
        let managerIds = remoteSearchManagerIds()
        guard !managerIds.isEmpty else { return }
        for managerId in managerIds {
            service()?.triggerRemoteSearchForManager(managerId: managerId, query: "") { taskId in
                if taskId < 0 {
                    logger.debug("warmup search for \(managerId) was not queued")
                }
            }
        }
    }

    func cancelActiveRemoteSearch() {
        let inFlightTaskIds = Set(
            activeTasks.compactMap { task -> Int64? in
                guard task.taskType?.lowercased() == "search", task.isRunning else { return nil }
                return Int64(task.id)
            }
        )
        let taskIdsToCancel = activeRemoteSearchTaskIds.union(inFlightTaskIds)
        activeRemoteSearchTaskIds = []
        isSearching = false

        for taskId in taskIdsToCancel {
            service()?.cancelTask(taskId: taskId) { success in
                if !success {
                    logger.warning("cancelTask(\(taskId)) returned false")
                }
            }
        }
    }

    func clearSearchState() {
        activeRemoteSearchTaskIds = []
        isSearching = false
    }

    func ensurePackageDescription(for package: PackageItem) {
        guard supportsRemoteSearch(managerId: package.managerId),
              managerStatuses[package.managerId]?.enabled ?? true else {
            packageDescriptionLoadingIds.remove(package.id)
            packageDescriptionUnavailableIds.insert(package.id)
            return
        }

        let hasCachedSummary = {
            guard let summary = package.summary?.trimmingCharacters(in: .whitespacesAndNewlines) else { return false }
            return !summary.isEmpty
        }()
        let now = Date()
        if hasCachedSummary,
           let lastAttempt = descriptionLookupLastAttemptByPackage[package.id],
           now.timeIntervalSince(lastAttempt) < 30 {
            packageDescriptionUnavailableIds.remove(package.id)
            return
        }
        if !hasCachedSummary && packageDescriptionLoadingIds.contains(package.id) {
            return
        }

        descriptionLookupLastAttemptByPackage[package.id] = now
        packageDescriptionUnavailableIds.remove(package.id)
        if !hasCachedSummary {
            packageDescriptionLoadingIds.insert(package.id)
        }

        service()?.triggerRemoteSearchForManager(managerId: package.managerId, query: package.name) { [weak self] taskId in
            DispatchQueue.main.async {
                guard let self = self else { return }
                if taskId < 0 {
                    self.packageDescriptionLoadingIds.remove(package.id)
                    if !hasCachedSummary {
                        self.packageDescriptionUnavailableIds.insert(package.id)
                    }
                    return
                }

                var taskIds = self.descriptionLookupTaskIdsByPackage[package.id] ?? Set<UInt64>()
                taskIds.insert(UInt64(taskId))
                self.descriptionLookupTaskIdsByPackage[package.id] = taskIds
                self.activeRemoteSearchTaskIds.insert(taskId)
            }
        }
    }

    func setPinnedState(packageId: String, pinned: Bool) {
        if let index = installedPackages.firstIndex(where: { $0.id == packageId }) {
            installedPackages[index].pinned = pinned
        }
        if let index = outdatedPackages.firstIndex(where: { $0.id == packageId }) {
            outdatedPackages[index].pinned = pinned
        }
        if let index = searchResults.firstIndex(where: { $0.id == packageId }) {
            searchResults[index].pinned = pinned
        }
        if let index = cachedAvailablePackages.firstIndex(where: { $0.id == packageId }) {
            cachedAvailablePackages[index].pinned = pinned
        }
    }
}
