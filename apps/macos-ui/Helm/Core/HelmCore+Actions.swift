import Foundation
import os.log

// swiftlint:disable file_length

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
                    self?.recordLastError(
                        source: "core.actions",
                        action: "cancelTask",
                        managerId: task.managerId,
                        taskType: task.taskType
                    )
                }
            }
        }
    }

    func dismissTask(_ task: TaskItem) {
        if task.id.hasPrefix(Self.localManagerActionTaskIdPrefix) {
            localManagerActionTasks.removeValue(forKey: task.id)
            localManagerActionTaskCreatedAt.removeValue(forKey: task.id)
            activeTasks.removeAll { $0.id == task.id }
            return
        }

        guard !task.isRunning,
              task.status.lowercased() == "failed",
              let taskId = Int64(task.id) else { return }

        service()?.dismissTask(taskId: taskId) { [weak self] success in
            DispatchQueue.main.async {
                guard let self else { return }
                if success {
                    self.activeTasks.removeAll { $0.id == task.id }
                } else {
                    logger.warning("dismissTask(\(taskId)) returned false")
                    self.recordLastError(
                        source: "core.actions",
                        action: "dismissTask",
                        managerId: task.managerId,
                        taskType: task.taskType
                    )
                }
            }
        }
    }

    func respondTaskTimeoutPrompt(taskId: UInt64, waitForCompletion: Bool, completion: ((Bool) -> Void)? = nil) {
        guard let service = service() else {
            completion?(false)
            return
        }

        service.respondTaskTimeoutPrompt(taskId: Int64(taskId), waitForCompletion: waitForCompletion) { [weak self] success in
            DispatchQueue.main.async {
                guard let self else {
                    completion?(success)
                    return
                }
                if success {
                    self.fetchTasks()
                    self.fetchTaskTimeoutPrompts()
                } else {
                    logger.warning(
                        "respondTaskTimeoutPrompt(\(taskId), wait=\(waitForCompletion)) returned false"
                    )
                    self.recordLastError(
                        source: "core.actions",
                        action: "respondTaskTimeoutPrompt",
                        taskType: "diagnostics"
                    )
                }
                completion?(success)
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
            recordLastError(
                source: "core.actions",
                action: "upgradePackage.service_unavailable",
                managerId: package.managerId,
                taskType: "upgrade"
            )
            DispatchQueue.main.async {
                self.upgradeActionPackageIds.remove(package.id)
            }
            return
        }

        withTimeout(
            300,
            source: "core.actions",
            action: "upgradePackage",
            managerId: package.managerId,
            taskType: "upgrade",
            operation: { completion in
            service.upgradePackage(
                managerId: package.managerId,
                packageName: package.mutationPackageName
            ) { completion($0) }
        }, fallback: Int64(-1)) { [weak self] taskId in
            DispatchQueue.main.async {
                guard let self = self, let taskId = taskId else { return }
                if taskId < 0 {
                    self.upgradeActionTaskByPackage.removeValue(forKey: package.id)
                    self.upgradeActionPackageIds.remove(package.id)
                    logger.error("upgradePackage(\(package.managerId):\(package.name)) failed")
                    self.recordLastError(
                        source: "core.actions",
                        action: "upgradePackage.queue_failed",
                        managerId: package.managerId,
                        taskType: "upgrade"
                    )
                    return
                }

                self.upgradeActionTaskByPackage[package.id] = UInt64(taskId)
                self.registerManagerActionTask(
                    managerId: package.managerId,
                    taskId: UInt64(taskId),
                    taskType: "package_upgrade",
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
                    self.recordLastError(
                        source: "core.actions",
                        action: "cancelTask",
                        taskType: "upgrade"
                    )
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
                self.recordLastError(
                    source: "core.actions",
                    action: "runUpgradePlanScoped.phase_timeout",
                    taskType: "upgrade"
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
            recordLastError(
                source: "core.actions",
                action: "retryUpgradePlanStep.service_unavailable",
                managerId: step.managerId,
                taskType: "upgrade"
            )
            completion?(false)
            return
        }

        withTimeout(
            300,
            source: "core.actions",
            action: "retryUpgradePlanStep",
            managerId: step.managerId,
            taskType: "upgrade",
            operation: { completion in
            service.upgradePackage(managerId: step.managerId, packageName: step.packageName) { completion($0) }
        }, fallback: Int64(-1)) { [weak self] taskId in
            DispatchQueue.main.async {
                guard let self = self, let taskId = taskId else {
                    completion?(false)
                    return
                }
                guard taskId >= 0 else {
                    logger.error("retryUpgradePlanStep(\(step.id)) failed: upgradePackage returned \(taskId)")
                    self.recordLastError(
                        source: "core.actions",
                        action: "retryUpgradePlanStep.queue_failed",
                        managerId: step.managerId,
                        taskType: "upgrade"
                    )
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

    func installPackage(_ package: PackageItem, includeAlternates: Bool = true) {
        let targetPackage: PackageItem?
        if canInstallPackage(package, includeAlternates: false) {
            targetPackage = package
        } else if includeAlternates {
            targetPackage = preferredInstallCandidate(for: package)
        } else {
            targetPackage = nil
        }

        guard let targetPackage,
              canInstallPackage(targetPackage, includeAlternates: false),
              !installActionPackageIds.contains(targetPackage.id) else { return }

        let normalizedTargetPackageIdentity = PackageActionTracking.normalizedPackageIdentityKey(
            name: targetPackage.name,
            version: targetPackage.version
        )
        guard !normalizedTargetPackageIdentity.isEmpty,
              !installActionInFlightPackageNames().contains(normalizedTargetPackageIdentity) else { return }

        DispatchQueue.main.async {
            self.installActionPackageIds.insert(targetPackage.id)
            self.installActionNormalizedNameByPackageId[targetPackage.id] = normalizedTargetPackageIdentity
            self.installActionTargetPackageById[targetPackage.id] = targetPackage
        }

        guard let service = service() else {
            logger.error(
                "installPackage(\(targetPackage.managerId):\(targetPackage.name)) failed: service unavailable"
            )
            recordLastError(
                source: "core.actions",
                action: "installPackage.service_unavailable",
                managerId: targetPackage.managerId,
                taskType: "install"
            )
            DispatchQueue.main.async {
                self.installActionPackageIds.remove(targetPackage.id)
                self.installActionNormalizedNameByPackageId.removeValue(forKey: targetPackage.id)
                self.installActionTargetPackageById.removeValue(forKey: targetPackage.id)
            }
            return
        }

        let installRequestPackageName = targetPackage.mutationPackageName

        withTimeout(
            300,
            source: "core.actions",
            action: "installPackage",
            managerId: targetPackage.managerId,
            taskType: "install",
            operation: { completion in
            service.installPackage(
                managerId: targetPackage.managerId,
                packageName: installRequestPackageName
            ) { completion($0) }
        }, fallback: Int64(-1)) { [weak self] taskId in
            DispatchQueue.main.async {
                guard let self = self, let taskId = taskId else { return }
                if taskId < 0 {
                    self.installActionTaskByPackage.removeValue(forKey: targetPackage.id)
                    self.installActionPackageIds.remove(targetPackage.id)
                    self.installActionNormalizedNameByPackageId.removeValue(forKey: targetPackage.id)
                    self.installActionTargetPackageById.removeValue(forKey: targetPackage.id)
                    logger.error("installPackage(\(targetPackage.managerId):\(targetPackage.name)) failed")
                    self.recordLastError(
                        source: "core.actions",
                        action: "installPackage.queue_failed",
                        managerId: targetPackage.managerId,
                        taskType: "install"
                    )
                    return
                }

                self.installActionTaskByPackage[targetPackage.id] = UInt64(taskId)
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
            recordLastError(
                source: "core.actions",
                action: "uninstallPackage.service_unavailable",
                managerId: package.managerId,
                taskType: "uninstall"
            )
            DispatchQueue.main.async {
                self.uninstallActionPackageIds.remove(package.id)
            }
            return
        }

        withTimeout(
            300,
            source: "core.actions",
            action: "uninstallPackage",
            managerId: package.managerId,
            taskType: "uninstall",
            operation: { completion in
            service.uninstallPackage(
                managerId: package.managerId,
                packageName: package.mutationPackageName
            ) { completion($0) }
        }, fallback: Int64(-1)) { [weak self] taskId in
            DispatchQueue.main.async {
                guard let self = self, let taskId = taskId else { return }
                if taskId < 0 {
                    self.uninstallActionTaskByPackage.removeValue(forKey: package.id)
                    self.uninstallActionPackageIds.remove(package.id)
                    logger.error("uninstallPackage(\(package.managerId):\(package.name)) failed")
                    self.recordLastError(
                        source: "core.actions",
                        action: "uninstallPackage.queue_failed",
                        managerId: package.managerId,
                        taskType: "uninstall"
                    )
                    return
                }

                self.uninstallActionTaskByPackage[package.id] = UInt64(taskId)
            }
        }
    }

    func pinPackage(_ package: PackageItem) {
        guard canPinPackage(package), !pinActionPackageIds.contains(package.id) else { return }
        DispatchQueue.main.async {
            self.pinActionPackageIds.insert(package.id)
        }
        guard let service = service() else {
            logger.error("pinPackage(\(package.managerId):\(package.name)) failed: service unavailable")
            recordLastError(
                source: "core.actions",
                action: "pinPackage.service_unavailable",
                managerId: package.managerId,
                taskType: "pin"
            )
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
                    self?.setPinnedState(for: package, pinned: true)
                    self?.schedulePinnedStateReconciliation()
                } else {
                    logger.error("pinPackage(\(package.managerId):\(package.name)) failed")
                    self?.recordLastError(
                        source: "core.actions",
                        action: "pinPackage",
                        managerId: package.managerId,
                        taskType: "pin"
                    )
                }
            }
        }
    }

    func unpinPackage(_ package: PackageItem) {
        guard canPinPackage(package), !pinActionPackageIds.contains(package.id) else { return }
        DispatchQueue.main.async {
            self.pinActionPackageIds.insert(package.id)
        }
        guard let service = service() else {
            logger.error("unpinPackage(\(package.managerId):\(package.name)) failed: service unavailable")
            recordLastError(
                source: "core.actions",
                action: "unpinPackage.service_unavailable",
                managerId: package.managerId,
                taskType: "unpin"
            )
            DispatchQueue.main.async {
                self.pinActionPackageIds.remove(package.id)
            }
            return
        }
        service.unpinPackage(managerId: package.managerId, packageName: package.name) { [weak self] success in
            DispatchQueue.main.async {
                self?.pinActionPackageIds.remove(package.id)
                if success {
                    self?.setPinnedState(for: package, pinned: false)
                    self?.schedulePinnedStateReconciliation()
                } else {
                    logger.error("unpinPackage(\(package.managerId):\(package.name)) failed")
                    self?.recordLastError(
                        source: "core.actions",
                        action: "unpinPackage",
                        managerId: package.managerId,
                        taskType: "unpin"
                    )
                }
            }
        }
    }

    func isRustupToolchainActionInFlight(for package: PackageItem) -> Bool {
        guard let prefix = rustupToolchainActionPrefix(for: package) else { return false }
        return rustupToolchainActionInFlightKeys.contains { $0.hasPrefix(prefix) }
    }

    func isRustupToolchainActionInFlight(for package: PackageItem, scope: String) -> Bool {
        guard let actionKey = rustupToolchainActionKey(for: package, scope: scope) else { return false }
        return rustupToolchainActionInFlightKeys.contains(actionKey)
    }

    func addRustupComponent(_ component: String, to package: PackageItem) {
        queueRustupToolchainAction(
            package: package,
            scope: "component:add:\(component)",
            action: "rustupAddComponent"
        ) { service, completion in
            service.addRustupComponent(toolchain: package.name, component: component, withReply: completion)
        }
    }

    func removeRustupComponent(_ component: String, from package: PackageItem) {
        queueRustupToolchainAction(
            package: package,
            scope: "component:remove:\(component)",
            action: "rustupRemoveComponent"
        ) { service, completion in
            service.removeRustupComponent(toolchain: package.name, component: component, withReply: completion)
        }
    }

    func addRustupTarget(_ target: String, to package: PackageItem) {
        queueRustupToolchainAction(
            package: package,
            scope: "target:add:\(target)",
            action: "rustupAddTarget"
        ) { service, completion in
            service.addRustupTarget(toolchain: package.name, target: target, withReply: completion)
        }
    }

    func removeRustupTarget(_ target: String, from package: PackageItem) {
        queueRustupToolchainAction(
            package: package,
            scope: "target:remove:\(target)",
            action: "rustupRemoveTarget"
        ) { service, completion in
            service.removeRustupTarget(toolchain: package.name, target: target, withReply: completion)
        }
    }

    func setRustupDefaultToolchain(_ package: PackageItem) {
        queueRustupToolchainAction(
            package: package,
            scope: "default",
            action: "setRustupDefaultToolchain"
        ) { service, completion in
            service.setRustupDefaultToolchain(toolchain: package.name, withReply: completion)
        }
    }

    func setRustupOverride(_ package: PackageItem, path: String) {
        queueRustupToolchainAction(
            package: package,
            scope: "override:set:\(path)",
            action: "setRustupOverride"
        ) { service, completion in
            service.setRustupOverride(toolchain: package.name, path: path, withReply: completion)
        }
    }

    func unsetRustupOverride(_ package: PackageItem, path: String) {
        queueRustupToolchainAction(
            package: package,
            scope: "override:unset:\(path)",
            action: "unsetRustupOverride"
        ) { service, completion in
            service.unsetRustupOverride(toolchain: package.name, path: path, withReply: completion)
        }
    }

    func setRustupProfile(_ profile: String, for package: PackageItem) {
        queueRustupToolchainAction(
            package: package,
            scope: "profile:\(profile)",
            action: "setRustupProfile"
        ) { service, completion in
            service.setRustupProfile(profile: profile, withReply: completion)
        }
    }

    func clearRustupToolchainActionTracking(for actionKey: String) {
        rustupToolchainActionInFlightKeys.remove(actionKey)
        rustupToolchainActionTaskByKey.removeValue(forKey: actionKey)
        rustupToolchainActionPackageByKey.removeValue(forKey: actionKey)
        rustupToolchainActionSubmittedAtByKey.removeValue(forKey: actionKey)
    }

    private func queueRustupToolchainAction(
        package: PackageItem,
        scope: String,
        action: String,
        operation: @escaping (HelmServiceProtocol, @escaping (Int64) -> Void) -> Void
    ) {
        guard package.managerId == "rustup", package.status != .available else { return }
        guard let actionKey = rustupToolchainActionKey(for: package, scope: scope) else { return }
        guard !rustupToolchainActionInFlightKeys.contains(actionKey) else { return }

        DispatchQueue.main.async {
            self.rustupToolchainActionInFlightKeys.insert(actionKey)
            self.rustupToolchainActionPackageByKey[actionKey] = package
        }

        guard let service = service() else {
            recordLastError(
                source: "core.actions",
                action: "\(action).service_unavailable",
                managerId: package.managerId,
                taskType: "configure"
            )
            DispatchQueue.main.async {
                self.clearRustupToolchainActionTracking(for: actionKey)
            }
            return
        }

        withTimeout(
            300,
            source: "core.actions",
            action: action,
            managerId: package.managerId,
            taskType: "configure",
            operation: { completion in
                operation(service) { completion($0) }
            },
            fallback: Int64(-1)
        ) { [weak self] taskId in
            DispatchQueue.main.async {
                guard let self, let taskId else { return }
                if taskId < 0 {
                    logger.error("\(action, privacy: .public)(\(package.name, privacy: .public)) failed")
                    self.consumeLastServiceErrorKey { serviceErrorKey in
                        self.recordLastError(
                            message: serviceErrorKey?.localized ?? L10n.Common.error.localized,
                            source: "core.actions",
                            action: "\(action).queue_failed",
                            managerId: package.managerId,
                            taskType: "configure"
                        )
                    }
                    self.clearRustupToolchainActionTracking(for: actionKey)
                    return
                }

                self.rustupToolchainActionTaskByKey[actionKey] = UInt64(taskId)
                self.rustupToolchainActionSubmittedAtByKey[actionKey] = Date()
            }
        }
    }

    private func rustupToolchainActionKey(for package: PackageItem, scope: String) -> String? {
        guard let prefix = rustupToolchainActionPrefix(for: package) else { return nil }
        let normalizedScope = scope.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !normalizedScope.isEmpty else { return nil }
        return prefix + normalizedScope
    }

    private func rustupToolchainActionPrefix(for package: PackageItem) -> String? {
        guard package.managerId == "rustup" else { return nil }
        let normalizedToolchain = package.name
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()
        guard !normalizedToolchain.isEmpty else { return nil }
        return "rustup|\(normalizedToolchain)|"
    }

    func setManagerEnabled(_ managerId: String, enabled: Bool, completion: ((Bool) -> Void)? = nil) {
        if isManagerUninstalling(managerId) {
            completion?(false)
            return
        }

        if enabled,
           let status = managerStatuses[managerId],
           status.isEligible == false
        {
            let ineligibleMessage = status.ineligibleServiceErrorKey?.localized
                ?? status.ineligibleReasonMessage
                ?? L10n.Common.error.localized
            recordLastError(
                message: ineligibleMessage,
                source: "core.actions",
                action: "setManagerEnabled.ineligible",
                managerId: managerId,
                taskType: "settings"
            )
            completion?(false)
            return
        }

        if enabled,
           let status = managerStatuses[managerId],
           status.packageStateIssues?.contains(where: { issue in
               issue.issueCode == "post_install_setup_required"
           }) == true
        {
            recordLastError(
                message: "service.error.manager_setup_required".localized,
                source: "core.actions",
                action: "setManagerEnabled.setup_required",
                managerId: managerId,
                taskType: "settings"
            )
            completion?(false)
            return
        }

        guard let service = service() else {
            recordLastError(
                source: "core.actions",
                action: "setManagerEnabled.service_unavailable",
                managerId: managerId,
                taskType: "settings"
            )
            completion?(false)
            return
        }

        service.setManagerEnabled(managerId: managerId, enabled: enabled) { [weak self] success in
            DispatchQueue.main.async {
                guard let self else { return }
                if !success {
                    logger.error("setManagerEnabled(\(managerId), \(enabled)) failed")
                    self.consumeLastServiceErrorKey { serviceErrorKey in
                        let errorMessage = serviceErrorKey?.localized ?? L10n.Common.error.localized
                        self.recordLastError(
                            message: errorMessage,
                            source: "core.actions",
                            action: "setManagerEnabled",
                            managerId: managerId,
                            taskType: "settings"
                        )
                    }
                    completion?(false)
                    return
                }

                if !enabled {
                    self.cancelInFlightTasks(for: managerId)
                    self.pruneDisabledManagerState(managerId: managerId)
                }

                self.fetchManagerStatus()
                self.fetchTasks()
                self.fetchPackages()
                self.fetchOutdatedPackages()
                self.refreshCachedAvailablePackages()
                self.refreshUpgradePlan(
                    includePinned: self.upgradePlanIncludePinned,
                    allowOsUpdates: self.upgradePlanAllowOsUpdates
                )
                completion?(true)
            }
        }
    }

    private func cancelInFlightTasks(for managerId: String) {
        let taskIds = Set(
            activeTasks.compactMap { task -> Int64? in
                guard task.managerId == managerId, task.isRunning else { return nil }
                return Int64(task.id)
            }
        )
        guard !taskIds.isEmpty else { return }

        for taskId in taskIds {
            service()?.cancelTask(taskId: taskId) { [weak self] success in
                guard success else {
                    logger.warning("cancelTask(\(taskId)) returned false while disabling \(managerId)")
                    self?.recordLastError(
                        source: "core.actions",
                        action: "cancelTask",
                        managerId: managerId,
                        taskType: "settings"
                    )
                    return
                }
            }
        }
    }

    private func pruneDisabledManagerState(managerId: String) {
        let packageIdPrefix = "\(managerId):"
        let removedStepIds = Set(
            upgradePlanSteps
                .filter { $0.managerId == managerId }
                .map(\.id)
        )

        installedPackages.removeAll { $0.managerId == managerId }
        outdatedPackages.removeAll { $0.managerId == managerId }
        searchResults.removeAll { $0.managerId == managerId }
        cachedAvailablePackages.removeAll { $0.managerId == managerId }
        activeTasks.removeAll { $0.managerId == managerId }

        managerOperations.removeValue(forKey: managerId)
        if let taskId = managerActionTaskByManager.removeValue(forKey: managerId) {
            managerActionTaskDescriptions.removeValue(forKey: taskId)
            managerActionTaskTypes.removeValue(forKey: taskId)
            managerActionTaskSubmittedAt.removeValue(forKey: taskId)
        }

        upgradeActionTaskByPackage = upgradeActionTaskByPackage.filter { !$0.key.hasPrefix(packageIdPrefix) }
        installActionTaskByPackage = installActionTaskByPackage.filter { !$0.key.hasPrefix(packageIdPrefix) }
        installActionTargetPackageById = installActionTargetPackageById.filter {
            !$0.key.hasPrefix(packageIdPrefix)
        }
        uninstallActionTaskByPackage = uninstallActionTaskByPackage.filter { !$0.key.hasPrefix(packageIdPrefix) }

        upgradeActionPackageIds = Set(upgradeActionPackageIds.filter { !$0.hasPrefix(packageIdPrefix) })
        installActionPackageIds = Set(installActionPackageIds.filter { !$0.hasPrefix(packageIdPrefix) })
        installActionNormalizedNameByPackageId = installActionNormalizedNameByPackageId.filter {
            !$0.key.hasPrefix(packageIdPrefix)
        }
        uninstallActionPackageIds = Set(uninstallActionPackageIds.filter { !$0.hasPrefix(packageIdPrefix) })
        pinActionPackageIds = Set(pinActionPackageIds.filter { !$0.hasPrefix(packageIdPrefix) })

        packageDescriptionLoadingIds = Set(packageDescriptionLoadingIds.filter { !$0.hasPrefix(packageIdPrefix) })
        packageDescriptionUnavailableIds = Set(
            packageDescriptionUnavailableIds.filter { !$0.hasPrefix(packageIdPrefix) }
        )
        descriptionLookupTaskIdsByPackage = descriptionLookupTaskIdsByPackage.filter {
            !$0.key.hasPrefix(packageIdPrefix)
        }
        descriptionLookupStartedAtByPackage = descriptionLookupStartedAtByPackage.filter {
            !$0.key.hasPrefix(packageIdPrefix)
        }
        descriptionLookupPackageById = descriptionLookupPackageById.filter {
            !$0.key.hasPrefix(packageIdPrefix)
        }
        let descriptionSummaryPrefix = "\(managerId.lowercased())|"
        packageDescriptionSummaryByKey = packageDescriptionSummaryByKey.filter {
            !$0.key.hasPrefix(descriptionSummaryPrefix)
        }
        let rustupPrefix = "\(managerId.lowercased())|"
        rustupToolchainDetailsByKey = rustupToolchainDetailsByKey.filter { !$0.key.hasPrefix(rustupPrefix) }
        rustupToolchainDetailLoadingKeys = Set(
            rustupToolchainDetailLoadingKeys.filter { !$0.hasPrefix(rustupPrefix) }
        )
        rustupToolchainDetailUnavailableKeys = Set(
            rustupToolchainDetailUnavailableKeys.filter { !$0.hasPrefix(rustupPrefix) }
        )
        rustupToolchainActionInFlightKeys = Set(
            rustupToolchainActionInFlightKeys.filter { !$0.hasPrefix(rustupPrefix) }
        )
        rustupToolchainActionTaskByKey = rustupToolchainActionTaskByKey.filter {
            !$0.key.hasPrefix(rustupPrefix)
        }
        rustupToolchainActionPackageByKey = rustupToolchainActionPackageByKey.filter {
            !$0.key.hasPrefix(rustupPrefix)
        }
        rustupToolchainActionSubmittedAtByKey = rustupToolchainActionSubmittedAtByKey.filter {
            !$0.key.hasPrefix(rustupPrefix)
        }

        if selectedManagerFilter == managerId {
            selectedManagerFilter = nil
        }

        if !removedStepIds.isEmpty {
            upgradePlanSteps.removeAll { $0.managerId == managerId }
            for stepId in removedStepIds {
                upgradePlanTaskProjectionByStepId.removeValue(forKey: stepId)
            }
            rebuildUpgradePlanFailureGroups()
        }
    }

    func setManagerSelectedExecutablePath(_ managerId: String, selectedPath: String?) {
        if isManagerUninstalling(managerId) {
            return
        }
        service()?.setManagerSelectedExecutablePath(managerId: managerId, selectedPath: selectedPath) { [weak self] success in
            guard let self else { return }
            if !success {
                logger.error("setManagerSelectedExecutablePath(\(managerId), \(selectedPath ?? "nil")) failed")
                self.recordLastError(
                    source: "core.actions",
                    action: "setManagerSelectedExecutablePath",
                    managerId: managerId,
                    taskType: "settings"
                )
                return
            }
            self.fetchManagerStatus()
        }
    }

    func setManagerActiveInstallInstance(
        _ managerId: String,
        instanceId: String,
        completion: ((Bool) -> Void)? = nil
    ) {
        if isManagerUninstalling(managerId) {
            completion?(false)
            return
        }
        service()?.setManagerActiveInstallInstance(managerId: managerId, instanceId: instanceId) { [weak self] success in
            guard let self else { return }
            if !success {
                logger.error("setManagerActiveInstallInstance(\(managerId), \(instanceId)) failed")
                self.recordLastError(
                    source: "core.actions",
                    action: "setManagerActiveInstallInstance",
                    managerId: managerId,
                    taskType: "settings"
                )
                DispatchQueue.main.async {
                    completion?(false)
                }
                return
            }
            self.fetchManagerStatus()
            self.triggerDetection(for: managerId)
            DispatchQueue.main.async {
                completion?(true)
            }
        }
    }

    func acknowledgeManagerMultiInstanceState(
        _ managerId: String,
        completion: ((Bool) -> Void)? = nil
    ) {
        service()?.acknowledgeManagerMultiInstanceState(managerId: managerId) { [weak self] success in
            guard let self else { return }
            if !success {
                logger.error("acknowledgeManagerMultiInstanceState(\(managerId)) failed")
                self.recordLastError(
                    source: "core.actions",
                    action: "acknowledgeManagerMultiInstanceState",
                    managerId: managerId,
                    taskType: "settings"
                )
            }
            self.fetchManagerStatus()
            DispatchQueue.main.async {
                completion?(success)
            }
        }
    }

    func clearManagerMultiInstanceAck(
        _ managerId: String,
        completion: ((Bool) -> Void)? = nil
    ) {
        service()?.clearManagerMultiInstanceAck(managerId: managerId) { [weak self] success in
            guard let self else { return }
            if !success {
                logger.error("clearManagerMultiInstanceAck(\(managerId)) failed")
                self.recordLastError(
                    source: "core.actions",
                    action: "clearManagerMultiInstanceAck",
                    managerId: managerId,
                    taskType: "settings"
                )
            }
            self.fetchManagerStatus()
            DispatchQueue.main.async {
                completion?(success)
            }
        }
    }

    func setManagerInstallMethod(
        _ managerId: String,
        installMethod: String?,
        completion: ((Bool) -> Void)? = nil
    ) {
        if isManagerUninstalling(managerId) {
            DispatchQueue.main.async {
                completion?(false)
            }
            return
        }
        guard let service = service() else {
            DispatchQueue.main.async {
                completion?(false)
            }
            return
        }
        service.setManagerInstallMethod(managerId: managerId, installMethod: installMethod) { [weak self] success in
            guard let self else { return }
            if !success {
                logger.error("setManagerInstallMethod(\(managerId), \(installMethod ?? "nil")) failed")
                self.recordLastError(
                    source: "core.actions",
                    action: "setManagerInstallMethod",
                    managerId: managerId,
                    taskType: "settings"
                )
                DispatchQueue.main.async {
                    completion?(false)
                }
                return
            }
            self.fetchManagerStatus()
            DispatchQueue.main.async {
                completion?(true)
            }
        }
    }

    func setManagerTimeoutProfile(
        _ managerId: String,
        hardTimeoutSeconds: Int?,
        idleTimeoutSeconds: Int?,
        completion: ((Bool) -> Void)? = nil
    ) {
        if isManagerUninstalling(managerId) {
            DispatchQueue.main.async {
                completion?(false)
            }
            return
        }
        let hardValue = Int64(hardTimeoutSeconds ?? 0)
        let idleValue = Int64(idleTimeoutSeconds ?? 0)
        guard let service = service() else {
            DispatchQueue.main.async {
                completion?(false)
            }
            return
        }
        service.setManagerTimeoutProfile(
            managerId: managerId,
            hardTimeoutSeconds: hardValue,
            idleTimeoutSeconds: idleValue
        ) { [weak self] success in
            guard let self else { return }
            if !success {
                logger.error(
                    "setManagerTimeoutProfile(\(managerId), hard=\(hardValue), idle=\(idleValue)) failed"
                )
                self.recordLastError(
                    source: "core.actions",
                    action: "setManagerTimeoutProfile",
                    managerId: managerId,
                    taskType: "settings"
                )
                DispatchQueue.main.async {
                    completion?(false)
                }
                return
            }
            self.fetchManagerStatus()
            DispatchQueue.main.async {
                completion?(true)
            }
        }
    }

    func installManager(
        _ managerId: String,
        options: ManagerInstallActionOptions? = nil
    ) {
        if isManagerUninstalling(managerId) {
            return
        }
        DispatchQueue.main.async {
            self.managerOperations[managerId] = L10n.App.Managers.Operation.startingInstall.localized
        }
        guard let svc = service() else {
            recordLastError(
                source: "core.actions",
                action: "installManager.service_unavailable",
                managerId: managerId,
                taskType: "install"
            )
            return
        }
        withTimeout(
            300,
            source: "core.actions",
            action: "installManager",
            managerId: managerId,
            taskType: "install",
            operation: { completion in
            let encodedOptions: String?
            if let options {
                do {
                    let data = try JSONEncoder().encode(options)
                    encodedOptions = String(data: data, encoding: .utf8)
                } catch {
                    logger.error(
                        "installManager(\(managerId)) failed to encode install options: \(error.localizedDescription)"
                    )
                    self.recordLastError(
                        source: "core.actions",
                        action: "installManager.options_encode_failed",
                        managerId: managerId,
                        taskType: "install"
                    )
                    completion(-1)
                    return
                }
            } else {
                encodedOptions = nil
            }
            svc.installManagerWithOptions(
                managerId: managerId,
                optionsJson: encodedOptions
            ) {
                completion($0)
            }
        }, fallback: Int64(-1)) { [weak self] taskId in
            DispatchQueue.main.async {
                guard let self = self, let taskId = taskId else { return }
                if taskId < 0 {
                    logger.error("installManager(\(managerId)) failed")
                    self.recordLastError(
                        source: "core.actions",
                        action: "installManager.queue_failed",
                        managerId: managerId,
                        taskType: "install"
                    )
                    self.consumeLastServiceErrorKey { serviceErrorKey in
                        self.managerOperations[managerId] =
                            serviceErrorKey?.localized ?? L10n.App.Managers.Operation.installFailed.localized
                        self.registerLocalManagerActionFailureTask(
                            managerId: managerId,
                            taskType: "install",
                            description: self.managerActionDescription(action: "Install", managerId: managerId)
                        )
                    }
                    return
                }
                self.registerManagerActionTask(
                    managerId: managerId,
                    taskId: UInt64(taskId),
                    taskType: "manager_install",
                    description: self.managerActionDescription(action: "Install", managerId: managerId),
                    inProgressText: L10n.App.Managers.Operation.installing.localized
                )
            }
        }
    }

    func applyManagerPackageStateIssueRepair(
        managerId: String,
        sourceManagerId: String,
        packageName: String,
        issueCode: String,
        optionId: String
    ) {
        if isManagerUninstalling(managerId) {
            return
        }
        let isManagerInstallRepair = optionId == "reinstall_manager_via_homebrew"
        let isManagerSetupRepair = optionId == "apply_post_install_setup_defaults"
        if isManagerInstallRepair {
            DispatchQueue.main.async {
                self.managerOperations[managerId] = L10n.App.Managers.Operation.startingInstall.localized
            }
        } else if isManagerSetupRepair {
            DispatchQueue.main.async {
                self.managerOperations[managerId] = L10n.App.Managers.Operation.verifying.localized
            }
        }
        guard let svc = service() else {
            recordLastError(
                source: "core.actions",
                action: "applyManagerPackageStateIssueRepair.service_unavailable",
                managerId: managerId,
                taskType: "repair"
            )
            return
        }

        withTimeout(
            300,
            source: "core.actions",
            action: "applyManagerPackageStateIssueRepair",
            managerId: managerId,
            taskType: "repair",
            operation: { completion in
                svc.applyManagerPackageStateIssueRepair(
                    managerId: managerId,
                    sourceManagerId: sourceManagerId,
                    packageName: packageName,
                    issueCode: issueCode,
                    optionId: optionId
                ) {
                    completion($0)
                }
            },
            fallback: Int64(-1)
        ) { [weak self] taskId in
            DispatchQueue.main.async {
                guard let self, let taskId else { return }
                if taskId < 0 {
                    logger.error(
                        "applyManagerPackageStateIssueRepair(\(managerId), \(issueCode), \(optionId)) failed"
                    )
                    self.recordLastError(
                        source: "core.actions",
                        action: "applyManagerPackageStateIssueRepair.queue_failed",
                        managerId: managerId,
                        taskType: "repair"
                    )
                    if isManagerInstallRepair {
                        self.consumeLastServiceErrorKey { serviceErrorKey in
                            self.managerOperations[managerId] =
                                serviceErrorKey?.localized
                                ?? L10n.App.Managers.Operation.installFailed.localized
                        }
                    } else if isManagerSetupRepair {
                        self.consumeLastServiceErrorKey { serviceErrorKey in
                            self.managerOperations[managerId] = serviceErrorKey?.localized
                                ?? L10n.Common.error.localized
                        }
                    }
                    return
                }

                if isManagerInstallRepair {
                    self.registerManagerActionTask(
                        managerId: managerId,
                        taskId: UInt64(taskId),
                        taskType: "manager_install",
                        description: self.managerActionDescription(
                            action: "Install",
                            managerId: managerId
                        ),
                        inProgressText: L10n.App.Managers.Operation.installing.localized
                    )
                } else if isManagerSetupRepair {
                    self.registerManagerActionTask(
                        managerId: managerId,
                        taskId: UInt64(taskId),
                        taskType: "manager_setup",
                        description: self.managerActionDescription(
                            action: "Finish Setup",
                            managerId: managerId
                        ),
                        inProgressText: L10n.App.Managers.Operation.verifying.localized
                    )
                }
            }
        }
    }

    func updateManager(_ managerId: String) {
        if isManagerUninstalling(managerId) {
            return
        }
        DispatchQueue.main.async {
            self.managerOperations[managerId] = L10n.App.Managers.Operation.startingUpdate.localized
        }
        guard let svc = service() else {
            recordLastError(
                source: "core.actions",
                action: "updateManager.service_unavailable",
                managerId: managerId,
                taskType: "upgrade"
            )
            return
        }
        withTimeout(
            300,
            source: "core.actions",
            action: "updateManager",
            managerId: managerId,
            taskType: "upgrade",
            operation: { completion in
            svc.updateManager(managerId: managerId) { completion($0) }
        }, fallback: Int64(-1)) { [weak self] taskId in
            DispatchQueue.main.async {
                guard let self = self, let taskId = taskId else { return }
                if taskId < 0 {
                    logger.error("updateManager(\(managerId)) failed")
                    self.recordLastError(
                        source: "core.actions",
                        action: "updateManager.queue_failed",
                        managerId: managerId,
                        taskType: "upgrade"
                    )
                    self.consumeLastServiceErrorKey { serviceErrorKey in
                        self.managerOperations[managerId] =
                            serviceErrorKey?.localized ?? L10n.App.Managers.Operation.updateFailed.localized
                    }
                    return
                }
                self.registerManagerActionTask(
                    managerId: managerId,
                    taskId: UInt64(taskId),
                    taskType: "manager_update",
                    description: self.managerActionDescription(action: "Update", managerId: managerId),
                    inProgressText: L10n.App.Managers.Operation.updating.localized
                )
            }
        }
    }

    func previewManagerUninstall(
        _ managerId: String,
        allowUnknownProvenance: Bool = false,
        completion: @escaping (ManagerUninstallPreview?) -> Void
    ) {
        previewManagerUninstall(
            managerId,
            options: ManagerUninstallActionOptions(
                allowUnknownProvenance: allowUnknownProvenance,
                homebrewCleanupMode: nil,
                miseCleanupMode: nil,
                miseConfigRemoval: nil,
                removeHelmManagedShellSetup: nil
            ),
            completion: completion
        )
    }

    func previewManagerUninstall(
        _ managerId: String,
        options: ManagerUninstallActionOptions?,
        completion: @escaping (ManagerUninstallPreview?) -> Void
    ) {
        guard let svc = service() else {
            recordLastError(
                source: "core.actions",
                action: "previewManagerUninstall.service_unavailable",
                managerId: managerId,
                taskType: "uninstall"
            )
            DispatchQueue.main.async {
                completion(nil)
            }
            return
        }

        withTimeout(
            120,
            source: "core.actions",
            action: "previewManagerUninstall",
            managerId: managerId,
            taskType: "uninstall",
            operation: { timeoutCompletion in
                let encodedOptions: String?
                if let options {
                    do {
                        let data = try JSONEncoder().encode(options)
                        encodedOptions = String(data: data, encoding: .utf8)
                    } catch {
                        logger.error(
                            "previewManagerUninstall(\(managerId)) failed to encode options: \(error.localizedDescription)"
                        )
                        self.recordLastError(
                            source: "core.actions",
                            action: "previewManagerUninstall.options_encode_failed",
                            managerId: managerId,
                            taskType: "uninstall"
                        )
                        timeoutCompletion(nil)
                        return
                    }
                } else {
                    encodedOptions = nil
                }
                svc.previewManagerUninstallWithOptions(
                    managerId: managerId,
                    optionsJson: encodedOptions
                ) { timeoutCompletion($0) }
            },
            fallback: String?.none
        ) { [weak self] jsonString in
            guard let self else { return }
            guard let jsonString,
                  let data = jsonString.data(using: .utf8),
                  let preview: ManagerUninstallPreview = self.decodeCorePayload(
                    ManagerUninstallPreview.self,
                    from: data,
                    decodeContext: "previewManagerUninstall",
                    source: "core.actions",
                    action: "previewManagerUninstall.decode",
                    managerId: managerId,
                    taskType: "uninstall"
                  ) else {
                DispatchQueue.main.async {
                    completion(nil)
                }
                return
            }

            DispatchQueue.main.async {
                completion(preview)
            }
        }
    }

    func previewPackageUninstall(
        _ package: PackageItem,
        completion: @escaping (PackageUninstallPreview?) -> Void
    ) {
        guard let svc = service() else {
            recordLastError(
                source: "core.actions",
                action: "previewPackageUninstall.service_unavailable",
                managerId: package.managerId,
                taskType: "uninstall"
            )
            DispatchQueue.main.async {
                completion(nil)
            }
            return
        }

        withTimeout(
            120,
            source: "core.actions",
            action: "previewPackageUninstall",
            managerId: package.managerId,
            taskType: "uninstall",
            operation: { timeoutCompletion in
                svc.previewPackageUninstall(
                    managerId: package.managerId,
                    packageName: package.mutationPackageName
                ) { timeoutCompletion($0) }
            },
            fallback: String?.none
        ) { [weak self] jsonString in
            guard let self else { return }
            guard let jsonString,
                  let data = jsonString.data(using: .utf8),
                  let preview: PackageUninstallPreview = self.decodeCorePayload(
                    PackageUninstallPreview.self,
                    from: data,
                    decodeContext: "previewPackageUninstall",
                    source: "core.actions",
                    action: "previewPackageUninstall.decode",
                    managerId: package.managerId,
                    taskType: "uninstall"
                  ) else {
                DispatchQueue.main.async {
                    completion(nil)
                }
                return
            }

            DispatchQueue.main.async {
                completion(preview)
            }
        }
    }

    func uninstallManager(_ managerId: String, allowUnknownProvenance: Bool = false) {
        uninstallManager(
            managerId,
            options: ManagerUninstallActionOptions(
                allowUnknownProvenance: allowUnknownProvenance,
                homebrewCleanupMode: nil,
                miseCleanupMode: nil,
                miseConfigRemoval: nil,
                removeHelmManagedShellSetup: nil
            )
        )
    }

    func uninstallManager(_ managerId: String, options: ManagerUninstallActionOptions?) {
        if isManagerUninstalling(managerId) {
            return
        }
        DispatchQueue.main.async {
            self.managerOperations[managerId] = L10n.App.Managers.Operation.startingUninstall.localized
        }
        guard let svc = service() else {
            recordLastError(
                source: "core.actions",
                action: "uninstallManager.service_unavailable",
                managerId: managerId,
                taskType: "uninstall"
            )
            return
        }
        withTimeout(
            300,
            source: "core.actions",
            action: "uninstallManager",
            managerId: managerId,
            taskType: "uninstall",
            operation: { completion in
                let encodedOptions: String?
                if let options {
                    do {
                        let data = try JSONEncoder().encode(options)
                        encodedOptions = String(data: data, encoding: .utf8)
                    } catch {
                        logger.error(
                            "uninstallManager(\(managerId)) failed to encode uninstall options: \(error.localizedDescription)"
                        )
                        self.recordLastError(
                            source: "core.actions",
                            action: "uninstallManager.options_encode_failed",
                            managerId: managerId,
                            taskType: "uninstall"
                        )
                        completion(-1)
                        return
                    }
                } else {
                    encodedOptions = nil
                }
                svc.uninstallManagerWithUninstallOptions(
                    managerId: managerId,
                    optionsJson: encodedOptions
                ) { completion($0) }
        }, fallback: Int64(-1)) { [weak self] taskId in
            DispatchQueue.main.async {
                guard let self = self, let taskId = taskId else { return }
                if taskId < 0 {
                    logger.error("uninstallManager(\(managerId)) failed")
                    self.recordLastError(
                        source: "core.actions",
                        action: "uninstallManager.queue_failed",
                        managerId: managerId,
                        taskType: "uninstall"
                    )
                    self.consumeLastServiceErrorKey { serviceErrorKey in
                        self.managerOperations[managerId] =
                            serviceErrorKey?.localized ?? L10n.App.Managers.Operation.uninstallFailed.localized
                    }
                    return
                }
                self.registerManagerActionTask(
                    managerId: managerId,
                    taskId: UInt64(taskId),
                    taskType: "manager_uninstall",
                    description: self.managerActionDescription(action: "Uninstall", managerId: managerId),
                    inProgressText: L10n.App.Managers.Operation.uninstalling.localized
                )
            }
        }
    }

    func verifyManagerPostInstallSetup(_ managerId: String) {
        _ = managerId
        fetchManagerStatus()
        fetchTasks()
        fetchPackages()
        fetchOutdatedPackages()
        refreshCachedAvailablePackages()
    }

    func registerManagerActionTask(
        managerId: String,
        taskId: UInt64,
        taskType: String,
        description: String,
        inProgressText: String
    ) {
        managerActionTaskDescriptions[taskId] = description
        managerActionTaskByManager[managerId] = taskId
        managerActionTaskTypes[taskId] = taskType
        managerActionTaskSubmittedAt[taskId] = Date()
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

    private func registerLocalManagerActionFailureTask(
        managerId: String,
        taskType: String?,
        description: String
    ) {
        let localTaskId = Self.localManagerActionTaskIdPrefix + UUID().uuidString
        let failedTask = TaskItem(
            id: localTaskId,
            description: description,
            status: "Failed",
            managerId: managerId,
            taskType: taskType,
            labelKey: nil,
            labelArgs: nil
        )

        localManagerActionTasks[localTaskId] = failedTask
        localManagerActionTaskCreatedAt[localTaskId] = Date()
        activeTasks.removeAll { $0.id == localTaskId }
        activeTasks.insert(failedTask, at: 0)
    }

    // MARK: - Search Orchestration

    func remoteSearchManagerIds() -> [String] {
        guard !managerStatuses.isEmpty else {
            if !detectedManagers.isEmpty {
                return ManagerInfo.all
                    .map(\.id)
                    .filter { supportsRemoteSearch(managerId: $0) }
                    .filter { isManagerDetected($0) }
            }
            return []
        }

        return ManagerInfo.all
            .map(\.id)
            .filter { supportsRemoteSearch(managerId: $0) }
            .filter { managerStatuses[$0]?.isImplemented ?? true }
            .filter { managerStatuses[$0]?.enabled ?? true }
            .filter { isManagerDetected($0) }
    }

    func onSearchTextChanged(_ query: String) {
        let normalizedQuery = query.trimmingCharacters(in: .whitespacesAndNewlines)

        // 1. Instant local cache query
        fetchSearchResults(query: normalizedQuery)

        // 2. Cancel in-flight remote search
        cancelActiveRemoteSearch()

        // 3. Invalidate debounce timer
        searchDebounceTimer?.invalidate()
        searchDebounceTimer = nil

        // 4. If empty, clear state and return
        guard !normalizedQuery.isEmpty else {
            isSearching = false
            return
        }

        // 5. Start 300ms debounce timer for remote search
        searchDebounceTimer = Timer.scheduledTimer(withTimeInterval: 0.3, repeats: false) { [weak self] _ in
            self?.triggerRemoteSearch(query: normalizedQuery)
        }
    }

    func triggerRemoteSearch(query: String) {
        let normalizedQuery = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !normalizedQuery.isEmpty else {
            isSearching = false
            return
        }

        let managerIds = remoteSearchManagerIds()
        guard !managerIds.isEmpty else {
            isSearching = false
            return
        }

        isSearching = true
        for managerId in managerIds {
            service()?.triggerRemoteSearchForManager(managerId: managerId, query: normalizedQuery) { [weak self] taskId in
                DispatchQueue.main.async {
                    guard let self = self else { return }
                    if taskId >= 0 {
                        self.activeRemoteSearchTaskIds.insert(taskId)
                    } else {
                        logger.warning("triggerRemoteSearchForManager(\(managerId)) returned error")
                        self.recordLastError(
                            source: "core.actions",
                            action: "triggerRemoteSearchForManager",
                            managerId: managerId,
                            taskType: "search"
                        )
                    }
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
                    self.recordLastError(
                        source: "core.actions",
                        action: "cancelTask",
                        taskType: "search"
                    )
                }
            }
        }
    }

    func clearSearchState() {
        activeRemoteSearchTaskIds = []
        isSearching = false
    }

    func ensurePackageDescription(for package: PackageItem) {
        final class DescriptionLookupSubmissionTracker {
            var remaining: Int
            var queuedTaskCount: Int = 0

            init(remaining: Int) {
                self.remaining = remaining
            }
        }

        descriptionLookupPackageById[package.id] = package
        let candidates = packageDescriptionLookupCandidates(for: package)
        guard !candidates.isEmpty else {
            descriptionLookupTaskIdsByPackage.removeValue(forKey: package.id)
            descriptionLookupStartedAtByPackage.removeValue(forKey: package.id)
            packageDescriptionLoadingIds.remove(package.id)
            packageDescriptionUnavailableIds.insert(package.id)
            return
        }

        let hasCachedSummary = packageDescriptionSummary(for: package) != nil
        if hasCachedSummary {
            packageDescriptionUnavailableIds.remove(package.id)
        }

        var missingByManager: [String: PackageDescriptionLookupCandidate] = [:]
        for candidate in candidates {
            if packageDescriptionSummaryByKey[candidate.lookupKey] != nil {
                continue
            }
            guard supportsRemoteSearch(managerId: candidate.managerId),
                  managerStatuses[candidate.managerId]?.enabled ?? true else {
                continue
            }
            missingByManager[candidate.managerId] = candidate
        }

        let lookupCandidates = missingByManager.values.sorted { lhs, rhs in
            let lhsPriority = managerPriorityRank(for: lhs.managerId)
            let rhsPriority = managerPriorityRank(for: rhs.managerId)
            if lhsPriority != rhsPriority {
                return lhsPriority < rhsPriority
            }
            return normalizedManagerName(lhs.managerId)
                .localizedCaseInsensitiveCompare(normalizedManagerName(rhs.managerId)) == .orderedAscending
        }

        guard !lookupCandidates.isEmpty else {
            descriptionLookupTaskIdsByPackage.removeValue(forKey: package.id)
            descriptionLookupStartedAtByPackage.removeValue(forKey: package.id)
            packageDescriptionLoadingIds.remove(package.id)
            if hasCachedSummary {
                packageDescriptionUnavailableIds.remove(package.id)
            } else {
                packageDescriptionUnavailableIds.insert(package.id)
            }
            return
        }

        if let inFlightTaskIds = descriptionLookupTaskIdsByPackage[package.id], !inFlightTaskIds.isEmpty {
            if descriptionLookupStartedAtByPackage[package.id] == nil {
                descriptionLookupStartedAtByPackage[package.id] = Date()
            }
            packageDescriptionLoadingIds.insert(package.id)
            return
        }

        guard let service = service() else {
            descriptionLookupTaskIdsByPackage.removeValue(forKey: package.id)
            descriptionLookupStartedAtByPackage.removeValue(forKey: package.id)
            packageDescriptionLoadingIds.remove(package.id)
            if hasCachedSummary {
                packageDescriptionUnavailableIds.remove(package.id)
            } else {
                packageDescriptionUnavailableIds.insert(package.id)
            }
            return
        }

        packageDescriptionUnavailableIds.remove(package.id)
        packageDescriptionLoadingIds.insert(package.id)
        descriptionLookupStartedAtByPackage[package.id] = Date()

        let tracker = DescriptionLookupSubmissionTracker(remaining: lookupCandidates.count)
        for candidate in lookupCandidates {
            service.triggerRemoteSearchForManager(managerId: candidate.managerId, query: package.name) { [weak self] taskId in
                DispatchQueue.main.async {
                    guard let self = self else { return }

                    if taskId >= 0 {
                        var taskIds = self.descriptionLookupTaskIdsByPackage[package.id] ?? Set<UInt64>()
                        taskIds.insert(UInt64(taskId))
                        self.descriptionLookupTaskIdsByPackage[package.id] = taskIds
                        self.activeRemoteSearchTaskIds.insert(taskId)
                        tracker.queuedTaskCount += 1
                    } else {
                        self.recordLastError(
                            source: "core.actions",
                            action: "ensurePackageDescription.triggerRemoteSearchForManager",
                            managerId: candidate.managerId,
                            taskType: "search"
                        )
                    }

                    tracker.remaining -= 1
                    guard tracker.remaining == 0 else { return }

                    if tracker.queuedTaskCount == 0 {
                        self.descriptionLookupTaskIdsByPackage.removeValue(forKey: package.id)
                        self.descriptionLookupStartedAtByPackage.removeValue(forKey: package.id)
                        self.packageDescriptionLoadingIds.remove(package.id)
                        if self.hasPackageDescriptionSummary(packageId: package.id) {
                            self.packageDescriptionUnavailableIds.remove(package.id)
                        } else {
                            self.packageDescriptionUnavailableIds.insert(package.id)
                        }
                    }
                }
            }
        }
    }

    func setPinnedState(for targetPackage: PackageItem, pinned: Bool) {
        installedPackages = applyingPinnedState(
            to: installedPackages,
            targetPackage: targetPackage,
            pinned: pinned
        )
        outdatedPackages = applyingPinnedState(
            to: outdatedPackages,
            targetPackage: targetPackage,
            pinned: pinned
        )
        searchResults = applyingPinnedState(
            to: searchResults,
            targetPackage: targetPackage,
            pinned: pinned
        )
        cachedAvailablePackages = applyingPinnedState(
            to: cachedAvailablePackages,
            targetPackage: targetPackage,
            pinned: pinned
        )
        objectWillChange.send()
    }

    private func schedulePinnedStateReconciliation() {
        // Pin state writes land in core storage asynchronously. Reconcile shortly after
        // optimistic updates so row badges + inspector actions stay in sync without
        // requiring an unrelated screen refresh.
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.35) { [weak self] in
            guard let self else { return }
            self.fetchPackages()
            self.fetchOutdatedPackages()
        }
    }

    private func applyingPinnedState(
        to packages: [PackageItem],
        targetPackage: PackageItem,
        pinned: Bool
    ) -> [PackageItem] {
        var changed = false
        let updated = packages.map { package -> PackageItem in
            guard shouldUpdatePinnedState(package: package, targetPackage: targetPackage),
                  package.pinned != pinned else { return package }
            changed = true
            var mutated = package
            mutated.pinned = pinned
            return mutated
        }
        return changed ? updated : packages
    }

    private func shouldUpdatePinnedState(package: PackageItem, targetPackage: PackageItem) -> Bool {
        if package.id == targetPackage.id {
            return true
        }

        guard package.managerId == targetPackage.managerId else {
            return false
        }

        let targetIdentityKey = targetPackage.normalizedIdentityKey
        let packageIdentityKey = package.normalizedIdentityKey
        if !targetIdentityKey.isEmpty || !packageIdentityKey.isEmpty {
            guard !targetIdentityKey.isEmpty, !packageIdentityKey.isEmpty else {
                return false
            }
            return packageIdentityKey == targetIdentityKey
        }

        // Fallback for managers that may emit unstable ids between views/refreshes.
        // Keep this scoped to non-available rows to avoid mutating catalog-only entries.
        guard package.status != .available, targetPackage.status != .available else {
            return false
        }
        return PackageIdentity.normalizedBaseName(package.name)
            == PackageIdentity.normalizedBaseName(targetPackage.name)
    }
}
