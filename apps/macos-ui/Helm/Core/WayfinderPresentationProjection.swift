import Foundation

enum WayfinderDestination: String, Codable, Equatable {
    case dashboard
    case plan
    case library
    case activity
    case environment
}

enum WayfinderFocusTarget: String, Codable, Equatable {
    case courseIndicator
    case primaryContent
    case selectedEntity
    case serviceHealth
}

struct WayfinderDeepLink: Codable, Equatable {
    let destination: WayfinderDestination
    let entityID: String?
    let focus: WayfinderFocusTarget
}

enum WayfinderCourseMode: String, Codable, Equatable {
    case healthy
    case updatesReady
    case determinateWork
    case indeterminateWork
    case approval
    case failedInterrupted
    case cachedPartialOffline
}

struct WayfinderLocalizedText: Equatable {
    let key: String
    let arguments: [String: String]

    init(key: String, arguments: [String: String] = [:]) {
        self.key = key
        self.arguments = arguments
    }
}

struct WayfinderDeterminateProgress: Equatable {
    let completed: Int
    let total: Int

    init?(completed: Int, total: Int) {
        guard total > 0, completed >= 0, completed <= total else { return nil }
        self.completed = completed
        self.total = total
    }

    var fraction: Double {
        Double(completed) / Double(total)
    }
}

enum WayfinderCondition: Equatable {
    case approvalRequired(count: Int)
    case failedOrInterrupted(failed: Int, interrupted: Int)
    case activeWork(count: Int)
    case actionableFinding(count: Int)
    case updatesReady(count: Int)
    case refreshing
    case serviceUnavailable
    case healthy

    var footerStatus: WayfinderFooterStatus {
        switch self {
        case .updatesReady:
            return .updatesAvailable
        case .activeWork, .refreshing:
            return .running
        case .failedOrInterrupted:
            return .error
        case .approvalRequired, .actionableFinding, .serviceUnavailable:
            return .attention
        case .healthy:
            return .healthy
        }
    }
}

enum WayfinderFooterStatus: Equatable {
    case healthy
    case updatesAvailable
    case running
    case attention
    case error

    var titleKey: String {
        switch self {
        case .healthy:
            return "app.health.healthy"
        case .updatesAvailable:
            return "app.health.updates_available"
        case .running:
            return "app.health.running"
        case .attention:
            return "app.health.attention"
        case .error:
            return "app.health.error"
        }
    }

    var icon: String {
        switch self {
        case .healthy:
            return "checkmark.circle.fill"
        case .updatesAvailable:
            return "arrow.up.circle.fill"
        case .running:
            return "arrow.triangle.2.circlepath"
        case .attention:
            return "exclamationmark.triangle.fill"
        case .error:
            return "xmark.octagon.fill"
        }
    }

    var isActionable: Bool {
        self != .healthy
    }
}

struct WayfinderCoverage: Equatable {
    let completed: Int
    let total: Int

    init?(completed: Int, total: Int) {
        guard total > 0, completed >= 0, completed <= total else { return nil }
        self.completed = completed
        self.total = total
    }
}

struct WayfinderProjectionContent: Equatable {
    let mode: WayfinderCourseMode
    let condition: WayfinderCondition
    let title: WayfinderLocalizedText
    let explanation: WayfinderLocalizedText
    let primaryActionTitle: WayfinderLocalizedText
    let primaryAction: WayfinderDeepLink
    let progress: WayfinderDeterminateProgress?
    let currentAuthorityStage: String?
    let freshnessDate: Date?
    let coverage: WayfinderCoverage?
}

struct WayfinderPresentationProjection: Equatable {
    let revision: UInt64
    let content: WayfinderProjectionContent

    static let initial = WayfinderPresentationProjection(
        revision: 0,
        content: WayfinderProjectionProjector.content(for: .healthy)
    )
}

struct WayfinderProjectionInput: Equatable {
    var serviceAvailable = true
    var approvalTaskIDs: [String] = []
    var failedTaskIDs: [String] = []
    var interruptedTaskIDs: [String] = []
    var activeTaskIDs: [String] = []
    var activeProgress: WayfinderDeterminateProgress?
    var currentAuthorityStage: String?
    var actionableFindingIDs: [String] = []
    var updateCount = 0
    var isRefreshing = false
    var freshnessDate: Date?
    var coverage: WayfinderCoverage?

    static let healthy = WayfinderProjectionInput()
}

enum WayfinderProjectionProjector {
    static func project(
        _ input: WayfinderProjectionInput,
        replacing previous: WayfinderPresentationProjection
    ) -> WayfinderPresentationProjection {
        let nextContent = content(for: input)
        guard nextContent != previous.content else { return previous }
        return WayfinderPresentationProjection(
            revision: previous.revision &+ 1,
            content: nextContent
        )
    }

    static func content(for input: WayfinderProjectionInput) -> WayfinderProjectionContent {
        let base: ProjectionBase

        if !input.approvalTaskIDs.isEmpty {
            base = ProjectionBase(
                mode: .approval,
                condition: .approvalRequired(count: input.approvalTaskIDs.count),
                titleKey: "app.wayfinder.course.approval.title",
                explanationKey: "app.wayfinder.course.approval.explanation",
                actionTitleKey: "app.wayfinder.action.review_activity",
                destination: .activity,
                entityID: input.approvalTaskIDs.first
            )
        } else if !input.failedTaskIDs.isEmpty || !input.interruptedTaskIDs.isEmpty {
            base = ProjectionBase(
                mode: .failedInterrupted,
                condition: .failedOrInterrupted(
                    failed: input.failedTaskIDs.count,
                    interrupted: input.interruptedTaskIDs.count
                ),
                titleKey: "app.wayfinder.course.failed.title",
                explanationKey: "app.wayfinder.course.failed.explanation",
                actionTitleKey: "app.wayfinder.action.review_activity",
                destination: .activity,
                entityID: input.failedTaskIDs.first ?? input.interruptedTaskIDs.first
            )
        } else if !input.activeTaskIDs.isEmpty {
            base = ProjectionBase(
                mode: input.activeProgress == nil ? .indeterminateWork : .determinateWork,
                condition: .activeWork(count: input.activeTaskIDs.count),
                titleKey: "app.wayfinder.course.active.title",
                explanationKey: "app.wayfinder.course.active.explanation",
                actionTitleKey: "app.wayfinder.action.view_activity",
                destination: .activity,
                entityID: input.activeTaskIDs.first
            )
        } else if !input.actionableFindingIDs.isEmpty {
            base = ProjectionBase(
                mode: .cachedPartialOffline,
                condition: .actionableFinding(count: input.actionableFindingIDs.count),
                titleKey: "app.wayfinder.course.finding.title",
                explanationKey: "app.wayfinder.course.finding.explanation",
                actionTitleKey: "app.wayfinder.action.review_environment",
                destination: .environment,
                entityID: input.actionableFindingIDs.first
            )
        } else if input.updateCount > 0 {
            base = ProjectionBase(
                mode: .updatesReady,
                condition: .updatesReady(count: input.updateCount),
                titleKey: "app.wayfinder.course.updates.title",
                explanationKey: "app.wayfinder.course.updates.explanation",
                actionTitleKey: "app.wayfinder.action.review_plan",
                destination: .plan,
                entityID: nil,
                arguments: ["count": String(input.updateCount)]
            )
        } else if input.isRefreshing {
            base = ProjectionBase(
                mode: .indeterminateWork,
                condition: .refreshing,
                titleKey: "app.wayfinder.course.refreshing.title",
                explanationKey: "app.wayfinder.course.refreshing.explanation",
                actionTitleKey: "app.wayfinder.action.open_dashboard",
                destination: .dashboard,
                entityID: nil
            )
        } else if !input.serviceAvailable {
            base = ProjectionBase(
                mode: .cachedPartialOffline,
                condition: .serviceUnavailable,
                titleKey: "app.wayfinder.course.unavailable.title",
                explanationKey: "app.wayfinder.course.unavailable.explanation",
                actionTitleKey: "app.inspector.view_diagnostics",
                destination: .dashboard,
                entityID: nil
            )
        } else {
            base = ProjectionBase(
                mode: .healthy,
                condition: .healthy,
                titleKey: "app.wayfinder.course.healthy.title",
                explanationKey: "app.wayfinder.course.healthy.explanation",
                actionTitleKey: "app.wayfinder.action.open_dashboard",
                destination: .dashboard,
                entityID: nil
            )
        }

        return WayfinderProjectionContent(
            mode: base.mode,
            condition: base.condition,
            title: WayfinderLocalizedText(key: base.titleKey, arguments: base.arguments),
            explanation: WayfinderLocalizedText(key: base.explanationKey, arguments: base.arguments),
            primaryActionTitle: WayfinderLocalizedText(key: base.actionTitleKey),
            primaryAction: WayfinderDeepLink(
                destination: base.destination,
                entityID: base.entityID,
                focus: base.condition == .serviceUnavailable
                    ? .serviceHealth
                    : (base.entityID == nil ? .primaryContent : .selectedEntity)
            ),
            progress: base.mode == .determinateWork ? input.activeProgress : nil,
            currentAuthorityStage: base.mode == .determinateWork || base.mode == .indeterminateWork
                ? input.currentAuthorityStage
                : nil,
            freshnessDate: input.freshnessDate,
            coverage: input.coverage
        )
    }

    private struct ProjectionBase {
        let mode: WayfinderCourseMode
        let condition: WayfinderCondition
        let titleKey: String
        let explanationKey: String
        let actionTitleKey: String
        let destination: WayfinderDestination
        let entityID: String?
        var arguments: [String: String] = [:]
    }
}
