import Foundation

enum MockDataFactory {
    static func makeSnapshot() -> HealthSnapshot {
        HealthSnapshot(
            aggregateStatus: .attention,
            pendingUpdates: 3,
            failures: 1,
            runningTasks: 2,
            lastRefresh: Date().addingTimeInterval(-140)
        )
    }

    static func makeManagers() -> [ManagerHealth] {
        [
            ManagerHealth(
                id: "mise",
                displayName: "mise",
                authority: .authoritative,
                status: .healthy,
                outdatedCount: 0,
                capabilitySummary: ["capability.list", "capability.outdated", "capability.upgrade"]
            ),
            ManagerHealth(
                id: "homebrew",
                displayName: "Homebrew",
                authority: .guarded,
                status: .attention,
                outdatedCount: 2,
                capabilitySummary: ["capability.list", "capability.outdated", "capability.install", "capability.upgrade", "capability.pin"]
            ),
            ManagerHealth(
                id: "npm",
                displayName: "npm",
                authority: .standard,
                status: .attention,
                outdatedCount: 1,
                capabilitySummary: ["capability.list", "capability.outdated", "capability.search", "capability.install", "capability.upgrade"]
            ),
            ManagerHealth(
                id: "softwareupdate",
                displayName: "softwareupdate",
                authority: .guarded,
                status: .healthy,
                outdatedCount: 0,
                capabilitySummary: ["capability.list", "capability.outdated", "capability.upgrade"]
            )
        ]
    }

    static func makePackages() -> [PackageRecord] {
        [
            PackageRecord(
                id: "pkg-eslint",
                managerID: "npm",
                managerDisplayName: "npm",
                name: "eslint",
                installedVersion: "8.56.0",
                latestVersion: "9.1.0",
                isPinned: false,
                sourceQuery: "eslint",
                cachedAt: Date().addingTimeInterval(-30)
            ),
            PackageRecord(
                id: "pkg-swiftformat",
                managerID: "homebrew",
                managerDisplayName: "Homebrew",
                name: "swiftformat",
                installedVersion: "0.53.0",
                latestVersion: "0.54.2",
                isPinned: true,
                sourceQuery: "swift",
                cachedAt: Date().addingTimeInterval(-52)
            ),
            PackageRecord(
                id: "pkg-ripgrep",
                managerID: "homebrew",
                managerDisplayName: "Homebrew",
                name: "ripgrep",
                installedVersion: "14.0.3",
                latestVersion: "14.0.3",
                isPinned: false,
                sourceQuery: "grep",
                cachedAt: Date().addingTimeInterval(-15)
            )
        ]
    }

    static func makeTasks() -> [TaskRecord] {
        [
            TaskRecord(
                id: "task-upgrade-eslint",
                managerID: "npm",
                managerDisplayName: "npm",
                title: "task.title.upgradeSingle",
                state: .running,
                createdAt: Date().addingTimeInterval(-20)
            ),
            TaskRecord(
                id: "task-refresh-homebrew",
                managerID: "homebrew",
                managerDisplayName: "Homebrew",
                title: "task.title.refreshManager",
                state: .queued,
                createdAt: Date().addingTimeInterval(-65)
            ),
            TaskRecord(
                id: "task-upgrade-homebrew-failed",
                managerID: "homebrew",
                managerDisplayName: "Homebrew",
                title: "task.title.upgradeHomebrew",
                state: .failed,
                createdAt: Date().addingTimeInterval(-220)
            )
        ]
    }
}
