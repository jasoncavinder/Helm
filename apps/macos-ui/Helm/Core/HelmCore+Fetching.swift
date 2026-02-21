import AppKit
import Foundation
import os.log

private let logger = Logger(subsystem: "com.jasoncavinder.Helm", category: "core.fetching")

extension HelmCore {

    func fetchPackages() {
        guard let svc = service() else { return }
        withTimeout(30, operation: { completion in
            svc.listInstalledPackages { completion($0) }
        }) { [weak self] jsonString in
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
                logger.error("fetchPackages: decode failed (\(data.count) bytes): \(error)")
                DispatchQueue.main.async { self?.lastError = L10n.Common.error.localized }
            }
        }
    }

    func fetchOutdatedPackages() {
        guard let svc = service() else { return }
        withTimeout(30, operation: { completion in
            svc.listOutdatedPackages { completion($0) }
        }) { [weak self] jsonString in
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
                    if let self = self, !self.upgradePlanSteps.isEmpty {
                        self.refreshUpgradePlan(
                            includePinned: self.upgradePlanIncludePinned,
                            allowOsUpdates: self.upgradePlanAllowOsUpdates
                        )
                    }
                }
            } catch {
                logger.error("fetchOutdatedPackages: decode failed (\(data.count) bytes): \(error)")
                DispatchQueue.main.async { self?.lastError = L10n.Common.error.localized }
            }
        }
    }

    // swiftlint:disable:next function_body_length
    func fetchTasks() {
        guard let svc = service() else { return }
        withTimeout(30, operation: { completion in
            svc.listTasks { completion($0) }
        }) { [weak self] jsonString in
            guard let jsonString = jsonString, let data = jsonString.data(using: String.Encoding.utf8) else { return }

            do {
                let decoder = JSONDecoder()
                decoder.keyDecodingStrategy = .convertFromSnakeCase
                let coreTasks = try decoder.decode([CoreTaskRecord].self, from: data)

                DispatchQueue.main.async {
                    self?.latestCoreTasksSnapshot = coreTasks
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
                            status: task.status.capitalized,
                            managerId: task.manager,
                            taskType: task.taskType,
                            labelKey: task.labelKey,
                            labelArgs: task.labelArgs
                        )
                    }
                    .sorted { $0.statusSortOrder < $1.statusSortOrder }
                    self?.syncManagerOperations(from: coreTasks)
                    self?.syncUpgradeActions(from: coreTasks)
                    self?.syncInstallActions(from: coreTasks)
                    self?.syncUninstallActions(from: coreTasks)
                    self?.syncUpgradePlanProjection(from: coreTasks)
                    self?.syncPackageDescriptionLookups(from: coreTasks)

                    // Announce new task failures to VoiceOver
                    let currentFailed = self?.activeTasks.filter({ $0.status.lowercased() == "failed" }).count ?? 0
                    if currentFailed > previousFailed {
                        let newFailures = currentFailed - previousFailed
                        self?.postAccessibilityAnnouncement(
                            "app.status_item.error".localized(with: ["count": newFailures])
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
                    self?.activeRemoteSearchTaskIds = inFlightSearchTaskIds
                    let hasQuery = !(self?.searchText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ?? true)
                    self?.isSearching = hasQuery && !inFlightInteractiveSearchTaskIds.isEmpty
                }
            } catch {
                logger.error("fetchTasks: decode failed (\(data.count) bytes): \(error)")
                DispatchQueue.main.async { self?.lastError = L10n.Common.error.localized }
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
                    let resolvedSummaryIds = Set(
                        results.compactMap { result -> String? in
                            guard let summary = result.summary?.trimmingCharacters(in: .whitespacesAndNewlines),
                                  !summary.isEmpty else {
                                return nil
                            }
                            return "\(result.sourceManager):\(result.name)"
                        }
                    )

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
                    self?.packageDescriptionUnavailableIds.subtract(resolvedSummaryIds)
                    self?.packageDescriptionLoadingIds.subtract(resolvedSummaryIds)
                }
            } catch {
                logger.error("fetchSearchResults: decode failed (\(data.count) bytes): \(error)")
                DispatchQueue.main.async {
                    self?.searchResults = []
                    self?.lastError = L10n.Common.error.localized
                }
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
                    let excludedIds = Set(self.installedPackages.map(\.id))
                        .union(self.outdatedPackages.map(\.id))
                    var dedupedById: [String: PackageItem] = [:]

                    for result in results {
                        let id = "\(result.sourceManager):\(result.name)"
                        guard !excludedIds.contains(id) else { continue }
                        let candidate = PackageItem(
                            id: id,
                            name: result.name,
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
                        lhs.name.localizedCaseInsensitiveCompare(rhs.name) == .orderedAscending
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
                }
            } catch {
                logger.error("refreshCachedAvailablePackages: decode failed (\(data.count) bytes): \(error)")
            }
        }
    }

    // MARK: - Manager Status

    func fetchManagerStatus() {
        guard let svc = service() else { return }
        withTimeout(30, operation: { completion in
            svc.listManagerStatus { completion($0) }
        }) { [weak self] jsonString in
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
                logger.error("fetchManagerStatus: decode failed (\(data.count) bytes): \(error)")
                DispatchQueue.main.async { self?.lastError = L10n.Common.error.localized }
            }
        }
    }

    func fetchTaskOutput(taskId: String, completion: @escaping (CoreTaskOutputRecord?) -> Void) {
        guard let numericTaskId = Int64(taskId), let svc = service() else {
            completion(nil)
            return
        }

        withTimeout(30, operation: { callback in
            svc.getTaskOutput(taskId: numericTaskId) { callback($0) }
        }) { jsonString in
            guard let jsonString = jsonString,
                  let data = jsonString.data(using: .utf8) else {
                completion(nil)
                return
            }

            do {
                let decoder = JSONDecoder()
                decoder.keyDecodingStrategy = .convertFromSnakeCase
                let output = try decoder.decode(CoreTaskOutputRecord.self, from: data)
                completion(output)
            } catch {
                logger.error("fetchTaskOutput: decode failed (\(data.count) bytes): \(error)")
                completion(nil)
            }
        }
    }

    func fetchTaskLogs(taskId: String, limit: Int, completion: @escaping ([CoreTaskLogRecord]?) -> Void) {
        guard let numericTaskId = Int64(taskId), let svc = service() else {
            completion(nil)
            return
        }

        let clampedLimit = max(limit, 0)
        if clampedLimit == 0 {
            completion([])
            return
        }

        withTimeout(30, operation: { callback in
            svc.listTaskLogs(taskId: numericTaskId, limit: Int64(clampedLimit)) { callback($0) }
        }) { jsonString in
            guard let jsonString = jsonString,
                  let data = jsonString.data(using: .utf8) else {
                completion(nil)
                return
            }

            do {
                let decoder = JSONDecoder()
                decoder.keyDecodingStrategy = .convertFromSnakeCase
                let logs = try decoder.decode([CoreTaskLogRecord].self, from: data)
                completion(logs)
            } catch {
                logger.error("fetchTaskLogs: decode failed (\(data.count) bytes): \(error)")
                completion(nil)
            }
        }
    }

}
