import Foundation

enum HelmAggregateStatus: String, CaseIterable {
    case healthy
    case attention
    case error
    case running

    var localizationKey: String {
        "status.\(rawValue)"
    }

    var symbolName: String {
        switch self {
        case .healthy:
            return "checkmark.circle.fill"
        case .attention:
            return "exclamationmark.triangle.fill"
        case .error:
            return "xmark.octagon.fill"
        case .running:
            return "arrow.triangle.2.circlepath"
        }
    }
}

enum HelmSection: String, CaseIterable, Identifiable {
    case overview
    case updates
    case packages
    case tasks
    case managers
    case settings

    var id: String { rawValue }

    var localizationKey: String {
        "section.\(rawValue)"
    }

    var symbolName: String {
        switch self {
        case .overview:
            return "speedometer"
        case .updates:
            return "square.and.arrow.down.on.square"
        case .packages:
            return "shippingbox.fill"
        case .tasks:
            return "checklist"
        case .managers:
            return "slider.horizontal.3"
        case .settings:
            return "gearshape"
        }
    }
}

enum AuthorityLevel: String, CaseIterable {
    case authoritative
    case standard
    case guarded

    var localizationKey: String {
        "authority.\(rawValue)"
    }
}

enum TaskState: String, CaseIterable {
    case queued
    case running
    case succeeded
    case partialFailure
    case failed
    case canceled

    var localizationKey: String {
        "task.state.\(rawValue)"
    }
}

struct HealthSnapshot {
    var aggregateStatus: HelmAggregateStatus
    var pendingUpdates: Int
    var failures: Int
    var runningTasks: Int
    var lastRefresh: Date
}

struct ManagerHealth: Identifiable, Hashable {
    let id: String
    let displayName: String
    let authority: AuthorityLevel
    let status: HelmAggregateStatus
    let outdatedCount: Int
    let capabilitySummary: [String]
}

struct PackageRecord: Identifiable, Hashable {
    let id: String
    let managerID: String
    let managerDisplayName: String
    let name: String
    let installedVersion: String
    let latestVersion: String
    let isPinned: Bool
    let sourceQuery: String
    let cachedAt: Date

    var hasUpdate: Bool {
        installedVersion != latestVersion
    }
}

struct TaskRecord: Identifiable, Hashable {
    let id: String
    let managerID: String
    let managerDisplayName: String
    let title: String
    let state: TaskState
    let createdAt: Date
}

struct ExecutionStage: Identifiable, Hashable {
    var id: AuthorityLevel { authority }
    let authority: AuthorityLevel
    let managerCount: Int
    let packageCount: Int
}
