import Foundation
import SwiftUI

@MainActor
final class AppStateStore: ObservableObject {
    @Published var selectedSection: HelmSection? = .overview
    @Published var selectedManagerID: String?
    @Published var selectedPackageID: String?
    @Published var searchQuery = ""
    @Published var isShowingUpgradeSheet = false
    @Published var isRefreshing = false

    @Published private(set) var snapshot: HealthSnapshot
    @Published private(set) var managers: [ManagerHealth]
    @Published private(set) var packages: [PackageRecord]
    @Published private(set) var tasks: [TaskRecord]

    init(
        snapshot: HealthSnapshot = MockDataFactory.makeSnapshot(),
        managers: [ManagerHealth] = MockDataFactory.makeManagers(),
        packages: [PackageRecord] = MockDataFactory.makePackages(),
        tasks: [TaskRecord] = MockDataFactory.makeTasks()
    ) {
        self.snapshot = snapshot
        self.managers = managers
        self.packages = packages
        self.tasks = tasks
    }

    var visiblePackages: [PackageRecord] {
        let filtered = packages.filter { package in
            guard !searchQuery.isEmpty else { return true }
            return package.name.localizedCaseInsensitiveContains(searchQuery)
                || package.managerDisplayName.localizedCaseInsensitiveContains(searchQuery)
        }

        return filtered.sorted { lhs, rhs in
            lhs.name.localizedStandardCompare(rhs.name) == .orderedAscending
        }
    }

    var executionStages: [ExecutionStage] {
        AuthorityLevel.allCases.map { authority in
            let groupedManagers = managers.filter { $0.authority == authority }
            let managerIDs = Set(groupedManagers.map(\.id))
            let packageCount = packages.filter { package in
                package.hasUpdate && managerIDs.contains(package.managerID)
            }.count

            return ExecutionStage(
                authority: authority,
                managerCount: groupedManagers.count,
                packageCount: packageCount
            )
        }
    }

    var selectedManager: ManagerHealth? {
        guard let selectedManagerID else { return nil }
        return managers.first { $0.id == selectedManagerID }
    }

    var selectedPackage: PackageRecord? {
        guard let selectedPackageID else { return nil }
        return packages.first { $0.id == selectedPackageID }
    }

    func refresh() {
        isRefreshing = true
        snapshot.lastRefresh = Date()
        snapshot.aggregateStatus = tasks.contains(where: { $0.state == .failed }) ? .attention : .healthy
        isRefreshing = false
    }

    func runUpgradeAll(dryRun: Bool) {
        let task = TaskRecord(
            id: UUID().uuidString,
            managerID: "system",
            managerDisplayName: "Helm",
            title: dryRun ? "task.title.dryRunPlan" : "task.title.runUpgradePlan",
            state: dryRun ? .succeeded : .running,
            createdAt: Date()
        )
        tasks.insert(task, at: 0)
        snapshot.runningTasks += dryRun ? 0 : 1
        isShowingUpgradeSheet = false
    }

    func update(package: PackageRecord) {
        let task = TaskRecord(
            id: UUID().uuidString,
            managerID: package.managerID,
            managerDisplayName: package.managerDisplayName,
            title: "task.title.updatePackage",
            state: .running,
            createdAt: Date()
        )
        tasks.insert(task, at: 0)
        selectedPackageID = package.id
    }

    func togglePin(packageID: String) {
        guard let index = packages.firstIndex(where: { $0.id == packageID }) else { return }
        let package = packages[index]
        let updated = PackageRecord(
            id: package.id,
            managerID: package.managerID,
            managerDisplayName: package.managerDisplayName,
            name: package.name,
            installedVersion: package.installedVersion,
            latestVersion: package.latestVersion,
            isPinned: !package.isPinned,
            sourceQuery: package.sourceQuery,
            cachedAt: package.cachedAt
        )
        packages[index] = updated
    }
}
