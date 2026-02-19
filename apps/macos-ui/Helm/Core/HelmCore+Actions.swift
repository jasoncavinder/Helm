import Foundation
import os.log

private let logger = Logger(subsystem: "com.jasoncavinder.Helm", category: "core.actions")

extension HelmCore {

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
                            managerId: task.managerId
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

    func canUpgradeIndividually(_ package: PackageItem) -> Bool {
        let upgradableManagers: Set<String> = ["homebrew_formula", "mise", "npm", "pip", "pipx", "cargo", "cargo_binstall", "rustup"]
        return package.status == .upgradable
            && upgradableManagers.contains(package.managerId)
            && !package.pinned
    }

    func upgradePackage(_ package: PackageItem) {
        guard canUpgradeIndividually(package) else { return }

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
                    managerId: managerId
                ),
                at: 0
            )
        }
    }

    // MARK: - Search Orchestration

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
        isSearching = true
        service()?.triggerRemoteSearch(query: query) { [weak self] taskId in
            DispatchQueue.main.async {
                if taskId >= 0 {
                    self?.activeRemoteSearchTaskId = taskId
                } else {
                    logger.error("triggerRemoteSearch returned error")
                    self?.isSearching = false
                }
            }
        }
    }

    func cancelActiveRemoteSearch() {
        guard let taskId = activeRemoteSearchTaskId else { return }
        activeRemoteSearchTaskId = nil
        isSearching = false
        service()?.cancelTask(taskId: taskId) { success in
            if !success {
                logger.warning("cancelTask(\(taskId)) returned false")
            }
        }
    }

    func clearSearchState() {
        activeRemoteSearchTaskId = nil
        isSearching = false
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
