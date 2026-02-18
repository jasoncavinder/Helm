import AppKit
import Foundation
import os.log

private let logger = Logger(subsystem: "com.jasoncavinder.Helm", category: "core.fetching")

extension HelmCore {

    func fetchPackages() {
        service()?.listInstalledPackages { [weak self] jsonString in
            guard let jsonString = jsonString, let data = jsonString.data(using: String.Encoding.utf8) else { return }

            do {
                let decoder = JSONDecoder()
                decoder.keyDecodingStrategy = .convertFromSnakeCase
                let corePackages = try decoder.decode([CoreInstalledPackage].self, from: data)

                DispatchQueue.main.async {
                    self?.installedPackages = corePackages.map { pkg in
                        PackageItem(
                            id: "\(pkg.package.manager):\(pkg.package.name)",
                            name: pkg.package.name,
                            version: pkg.installedVersion ?? L10n.Common.unknown.localized,
                            managerId: pkg.package.manager,
                            manager: self?.normalizedManagerName(pkg.package.manager) ?? pkg.package.manager,
                            pinned: pkg.pinned
                        )
                    }
                }
            } catch {
                logger.error("Failed to decode packages: \(error)")
            }
        }
    }

    func fetchOutdatedPackages() {
        service()?.listOutdatedPackages { [weak self] jsonString in
            guard let jsonString = jsonString, let data = jsonString.data(using: String.Encoding.utf8) else { return }

            do {
                let decoder = JSONDecoder()
                decoder.keyDecodingStrategy = .convertFromSnakeCase
                let corePackages = try decoder.decode([CoreOutdatedPackage].self, from: data)

                DispatchQueue.main.async {
                    self?.outdatedPackages = corePackages.map { pkg in
                        PackageItem(
                            id: "\(pkg.package.manager):\(pkg.package.name)",
                            name: pkg.package.name,
                            version: pkg.installedVersion ?? L10n.Common.unknown.localized,
                            latestVersion: pkg.candidateVersion,
                            managerId: pkg.package.manager,
                            manager: self?.normalizedManagerName(pkg.package.manager) ?? pkg.package.manager,
                            pinned: pkg.pinned,
                            restartRequired: pkg.restartRequired
                        )
                    }
                }
            } catch {
                logger.error("Failed to decode outdated packages: \(error)")
            }
        }
    }

    // swiftlint:disable:next function_body_length
    func fetchTasks() {
        service()?.listTasks { [weak self] jsonString in
            guard let jsonString = jsonString, let data = jsonString.data(using: String.Encoding.utf8) else { return }

            do {
                let decoder = JSONDecoder()
                decoder.keyDecodingStrategy = .convertFromSnakeCase
                let coreTasks = try decoder.decode([CoreTaskRecord].self, from: data)

                DispatchQueue.main.async {
                    if let maxTaskId = coreTasks.map(\.id).max() {
                        self?.lastObservedTaskId = max(self?.lastObservedTaskId ?? 0, maxTaskId)
                    }

                    let previousFailed = self?.previousFailedTaskCount ?? 0

                    self?.activeTasks = coreTasks.map { task in
                        let overrideDescription = self?.managerActionTaskDescriptions[task.id]
                        let managerName = self?.normalizedManagerName(task.manager) ?? task.manager
                        let taskLabel = self?.localizedTaskLabel(from: task)
                        return TaskItem(
                            id: "\(task.id)",
                            description: overrideDescription
                                ?? taskLabel
                                ?? L10n.App.Tasks.fallbackDescription.localized(with: [
                                    "task_type": self?.localizedTaskType(task.taskType) ?? task.taskType.capitalized,
                                    "manager": managerName
                                ]),
                            status: task.status.capitalized
                        )
                    }
                    self?.syncManagerOperations(from: coreTasks)
                    self?.syncUpgradeActions(from: coreTasks)

                    // Announce new task failures to VoiceOver
                    let currentFailed = self?.activeTasks.filter({ $0.status.lowercased() == "failed" }).count ?? 0
                    if currentFailed > previousFailed {
                        let newFailures = currentFailed - previousFailed
                        self?.postAccessibilityAnnouncement(
                            "app.redesign.status_item.error".localized(with: ["count": newFailures])
                        )
                    }
                    self?.previousFailedTaskCount = currentFailed

                    // Derive detection status from Detection-type tasks specifically.
                    // Tasks are ordered most-recent-first. A manager is "detected" if
                    // its latest detection task completed successfully.
                    var latestDetectionByManager: [String: String] = [:]
                    for task in coreTasks {
                        guard task.taskType.lowercased() == "detection" else { continue }
                        if latestDetectionByManager[task.manager] == nil {
                            latestDetectionByManager[task.manager] = task.status.lowercased()
                        }
                    }
                    var detected = Set<String>()
                    for (manager, status) in latestDetectionByManager {
                        if status == "completed" {
                            detected.insert(manager)
                        }
                    }
                    self?.detectedManagers = detected
                    self?.updateOnboardingDetectionProgress(from: coreTasks)

                    let isRunning = coreTasks.contains {
                        let type = $0.taskType.lowercased()
                        let status = $0.status.lowercased()
                        return (type == "refresh" || type == "detection") &&
                               (status == "running" || status == "queued")
                    }

                    // Only show "refreshing" when we triggered a refresh this session.
                    // Without this guard, stale running tasks from a previous session
                    // would permanently lock isRefreshing = true.
                    let wasRefreshing = self?.previousRefreshState ?? false
                    if let lastTrigger = self?.lastRefreshTrigger {
                        if Date().timeIntervalSince(lastTrigger) > 120.0 {
                            // Safety valve: clear stuck refresh after 2 minutes
                            self?.isRefreshing = false
                            self?.lastRefreshTrigger = nil
                        } else if isRunning {
                            self?.isRefreshing = true
                        } else if Date().timeIntervalSince(lastTrigger) < 2.0 {
                            self?.isRefreshing = true
                        } else {
                            self?.isRefreshing = false
                            self?.lastRefreshTrigger = nil
                        }
                    } else {
                        self?.isRefreshing = false
                    }

                    // Announce refresh completion to VoiceOver
                    let nowRefreshing = self?.isRefreshing ?? false
                    if wasRefreshing && !nowRefreshing {
                        self?.postAccessibilityAnnouncement(
                            L10n.Common.success.localized
                        )
                    }
                    self?.previousRefreshState = nowRefreshing

                    // Detect remote search completion
                    if let searchTaskId = self?.activeRemoteSearchTaskId {
                        let matchingTask = coreTasks.first { $0.id == UInt64(searchTaskId) }
                        if let task = matchingTask {
                            let status = task.status.lowercased()
                            if status == "completed" || status == "failed" || status == "cancelled" {
                                self?.activeRemoteSearchTaskId = nil
                                self?.isSearching = false
                            }
                        }
                    }
                }
            } catch {
                logger.error("Failed to decode tasks: \(error)")
            }
        }
    }

    func fetchSearchResults(query: String) {
        guard !query.trimmingCharacters(in: .whitespaces).isEmpty else {
            DispatchQueue.main.async {
                self.searchResults = []
            }
            return
        }

        service()?.searchLocal(query: query) { [weak self] jsonString in
            guard let jsonString = jsonString, let data = jsonString.data(using: .utf8) else {
                DispatchQueue.main.async { self?.searchResults = [] }
                return
            }

            do {
                let decoder = JSONDecoder()
                decoder.keyDecodingStrategy = .convertFromSnakeCase
                let results = try decoder.decode([CoreSearchResult].self, from: data)

                DispatchQueue.main.async {
                    self?.searchResults = results.map { result in
                        PackageItem(
                            id: "\(result.sourceManager):\(result.name)",
                            name: result.name,
                            version: result.version ?? "",
                            managerId: result.sourceManager,
                            manager: self?.normalizedManagerName(result.sourceManager) ?? result.sourceManager,
                            summary: result.summary,
                            status: .available
                        )
                    }
                }
            } catch {
                logger.error("Failed to decode search results: \(error)")
                DispatchQueue.main.async { self?.searchResults = [] }
            }
        }
    }

    func refreshCachedAvailablePackages() {
        service()?.searchLocal(query: "") { [weak self] jsonString in
            guard let jsonString = jsonString, let data = jsonString.data(using: .utf8) else { return }

            do {
                let decoder = JSONDecoder()
                decoder.keyDecodingStrategy = .convertFromSnakeCase
                let results = try decoder.decode([CoreSearchResult].self, from: data)

                DispatchQueue.main.async {
                    guard let self = self else { return }
                    let installedIds = Set(self.installedPackages.map { $0.id })
                    self.cachedAvailablePackages = results.compactMap { result in
                        let id = "\(result.sourceManager):\(result.name)"
                        guard !installedIds.contains(id) else { return nil }
                        return PackageItem(
                            id: id,
                            name: result.name,
                            version: result.version ?? "",
                            managerId: result.sourceManager,
                            manager: self.normalizedManagerName(result.sourceManager),
                            summary: result.summary,
                            status: .available
                        )
                    }
                }
            } catch {
                logger.error("Failed to decode cached available packages: \(error)")
            }
        }
    }

    // MARK: - Manager Status

    func fetchManagerStatus() {
        service()?.listManagerStatus { [weak self] jsonString in
            guard let jsonString = jsonString, let data = jsonString.data(using: .utf8) else { return }

            do {
                let decoder = JSONDecoder()
                let statuses = try decoder.decode([ManagerStatus].self, from: data)

                DispatchQueue.main.async {
                    var map: [String: ManagerStatus] = [:]
                    for status in statuses {
                        map[status.managerId] = status
                    }
                    self?.managerStatuses = map
                    self?.pruneOnboardingDetectionForDisabledManagers()
                }
            } catch {
                logger.error("Failed to decode manager statuses: \(error)")
            }
        }
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
            logger.warning("Onboarding detection timed out waiting for managers: \(pending)")
            completeOnboardingDetectionProgress()
        }
    }
}
