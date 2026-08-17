import Foundation

enum WayfinderPopoverLayout {
    static let width: CGFloat = 400
    static let ordinaryHeight: CGFloat = 458
    static let onboardingHeight: CGFloat = 620
}

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

enum WayfinderConditionKind: String, Codable, Equatable {
    case approvalRequired
    case failedOrInterrupted
    case activeWork
    case actionableFinding
    case updatesReady
    case offline
    case refreshing
    case serviceUnavailable
    case healthy
}

struct WayfinderDeepLink: Codable, Equatable {
    let destination: WayfinderDestination
    let entityID: String?
    let focus: WayfinderFocusTarget
    let routeStage: WayfinderPopoverRouteStage?
    let originatingCondition: WayfinderConditionKind?

    init(
        destination: WayfinderDestination,
        entityID: String?,
        focus: WayfinderFocusTarget,
        routeStage: WayfinderPopoverRouteStage? = nil,
        originatingCondition: WayfinderConditionKind? = nil
    ) {
        self.destination = destination
        self.entityID = entityID
        self.focus = focus
        self.routeStage = routeStage
        self.originatingCondition = originatingCondition
    }
}

struct WayfinderNavigationState: Equatable {
    private(set) var deepLink: WayfinderDeepLink?

    mutating func record(_ deepLink: WayfinderDeepLink) {
        self.deepLink = deepLink
    }
}

struct WayfinderEnvironmentRouteFilterState: Equatable {
    private(set) var stage: WayfinderPopoverRouteStage?

    mutating func apply(_ deepLink: WayfinderDeepLink) {
        stage = deepLink.destination == .environment ? deepLink.routeStage : nil
    }

    mutating func clear() {
        stage = nil
    }
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

struct DashboardManagerCounts: Equatable {
    let detected: Int
    let disabled: Int
    let available: Int

    init(statuses: [(detected: Bool, enabled: Bool, isImplemented: Bool)]) {
        let implemented = statuses.filter { $0.isImplemented }
        detected = implemented.filter { $0.detected }.count
        disabled = implemented.filter { $0.detected && !$0.enabled }.count
        available = implemented.filter { !$0.detected }.count
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
    case offline
    case refreshing
    case serviceUnavailable
    case healthy

    var kind: WayfinderConditionKind {
        switch self {
        case .approvalRequired:
            return .approvalRequired
        case .failedOrInterrupted:
            return .failedOrInterrupted
        case .activeWork:
            return .activeWork
        case .actionableFinding:
            return .actionableFinding
        case .updatesReady:
            return .updatesReady
        case .offline:
            return .offline
        case .refreshing:
            return .refreshing
        case .serviceUnavailable:
            return .serviceUnavailable
        case .healthy:
            return .healthy
        }
    }

    var footerStatus: WayfinderFooterStatus {
        switch self {
        case .updatesReady:
            return .updatesReady
        case .activeWork, .refreshing:
            return .running
        case .failedOrInterrupted:
            return .error
        case .approvalRequired, .actionableFinding:
            return .needsReview
        case .offline, .serviceUnavailable:
            return .unavailable
        case .healthy:
            return .healthy
        }
    }

    var sidebarFooterStatus: WayfinderFooterStatus? {
        switch self {
        case .updatesReady:
            return nil
        default:
            return footerStatus
        }
    }
}

enum WayfinderFooterStatus: Equatable {
    case healthy
    case updatesReady
    case running
    case needsReview
    case error
    case unavailable

    var titleKey: String {
        switch self {
        case .healthy:
            return "app.health.healthy"
        case .updatesReady:
            return "app.health.updates_ready"
        case .running:
            return "app.health.running"
        case .needsReview:
            return "app.health.needs_review"
        case .error:
            return "app.health.error"
        case .unavailable:
            return "app.health.unavailable"
        }
    }

    var icon: String {
        switch self {
        case .healthy:
            return "checkmark.circle.fill"
        case .updatesReady:
            return "arrow.up.circle.fill"
        case .running:
            return "arrow.triangle.2.circlepath"
        case .needsReview:
            return "exclamationmark.triangle.fill"
        case .error:
            return "xmark.octagon.fill"
        case .unavailable:
            return "minus.circle.fill"
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

enum WayfinderPopoverRouteStage: String, CaseIterable, Hashable, Codable {
    case system
    case tools
    case apps
    case packages

    var titleKey: String {
        "app.popover.wayfinder.route.\(rawValue)"
    }

    var symbol: String {
        switch self {
        case .system:
            return "desktopcomputer"
        case .tools:
            return "hammer"
        case .apps:
            return "app.dashed"
        case .packages:
            return "shippingbox"
        }
    }

    static func stage(forManagerCategory category: String) -> WayfinderPopoverRouteStage? {
        switch category {
        case "Toolchain":
            return .tools
        case "System/OS", "Security/Firmware":
            return .system
        case "App Store", "Container/VM":
            return .apps
        case "Language":
            return .packages
        default:
            return nil
        }
    }
}

enum WayfinderPopoverRouteTone: Equatable {
    case current
    case updates
    case active
    case review
    case error
    case pending
    case cached
}

struct WayfinderPopoverRouteItem: Equatable {
    let stage: WayfinderPopoverRouteStage
    let tone: WayfinderPopoverRouteTone
    let managerID: String?

    init(
        stage: WayfinderPopoverRouteStage,
        tone: WayfinderPopoverRouteTone,
        managerID: String? = nil
    ) {
        self.stage = stage
        self.tone = tone
        self.managerID = managerID
    }

    func deepLink(
        originatingCondition: WayfinderConditionKind
    ) -> WayfinderDeepLink {
        WayfinderDeepLink(
            destination: .environment,
            entityID: managerID,
            focus: managerID == nil ? .primaryContent : .selectedEntity,
            routeStage: stage,
            originatingCondition: originatingCondition
        )
    }
}

struct WayfinderPopoverFindingContext: Equatable {
    let title: WayfinderLocalizedText
    let detail: WayfinderLocalizedText
}

struct WayfinderPopoverPresentation: Equatable {
    let projection: WayfinderProjectionContent
    let routeItems: [WayfinderPopoverRouteItem]
    let contextTitle: WayfinderLocalizedText
    let contextDetail: WayfinderLocalizedText
    let contextSymbol: String
    let showsPrimaryAction: Bool
    let primaryActionTitle: WayfinderLocalizedText
    let allowsRefresh: Bool
}

struct WayfinderPopoverPresentationInput: Equatable {
    let projection: WayfinderProjectionContent
    var relatedRouteStages: [WayfinderPopoverRouteStage] = []
    var relatedManagerIDsByStage: [WayfinderPopoverRouteStage: String] = [:]
    var detectedManagerCount = 0
    var findingContext: WayfinderPopoverFindingContext?
}

enum WayfinderPopoverPresentationProjector {
    static func content(
        for input: WayfinderPopoverPresentationInput
    ) -> WayfinderPopoverPresentation {
        let condition = input.projection.condition
        let affectedStages = Set(input.relatedRouteStages)
        let routeItems = WayfinderPopoverRouteStage.allCases.map { stage in
            WayfinderPopoverRouteItem(
                stage: stage,
                tone: routeTone(
                    for: stage,
                    condition: condition,
                    affectedStages: affectedStages
                ),
                managerID: input.relatedManagerIDsByStage[stage]
            )
        }

        let context = contextContent(
            for: condition,
            detectedManagerCount: input.detectedManagerCount,
            projection: input.projection,
            findingContext: input.findingContext
        )

        return WayfinderPopoverPresentation(
            projection: input.projection,
            routeItems: routeItems,
            contextTitle: context.title,
            contextDetail: context.detail,
            contextSymbol: context.symbol,
            showsPrimaryAction: condition != .healthy && condition != .refreshing,
            primaryActionTitle: primaryActionTitle(
                for: condition,
                fallback: input.projection.primaryActionTitle
            ),
            allowsRefresh: condition != .offline && condition != .serviceUnavailable
        )
    }

    private static func routeTone(
        for stage: WayfinderPopoverRouteStage,
        condition: WayfinderCondition,
        affectedStages: Set<WayfinderPopoverRouteStage>
    ) -> WayfinderPopoverRouteTone {
        switch condition {
        case .offline, .serviceUnavailable:
            return .cached
        case .refreshing:
            return .pending
        case .healthy:
            return .current
        case .updatesReady:
            return affectedStages.contains(stage) ? .updates : .current
        case .activeWork:
            return affectedStages.contains(stage) ? .active : .current
        case .approvalRequired, .actionableFinding:
            return affectedStages.contains(stage) ? .review : .current
        case .failedOrInterrupted:
            return affectedStages.contains(stage) ? .error : .current
        }
    }

    private static func primaryActionTitle(
        for condition: WayfinderCondition,
        fallback: WayfinderLocalizedText
    ) -> WayfinderLocalizedText {
        switch condition {
        case .offline:
            return WayfinderLocalizedText(key: "app.popover.wayfinder.action.view_saved_state")
        case .failedOrInterrupted:
            return WayfinderLocalizedText(key: "app.popover.wayfinder.action.review_recovery")
        default:
            return fallback
        }
    }

    private static func contextContent(
        for condition: WayfinderCondition,
        detectedManagerCount: Int,
        projection: WayfinderProjectionContent,
        findingContext: WayfinderPopoverFindingContext?
    ) -> (title: WayfinderLocalizedText, detail: WayfinderLocalizedText, symbol: String) {
        switch condition {
        case .healthy:
            return (
                WayfinderLocalizedText(key: "app.popover.wayfinder.context.environment_covered"),
                WayfinderLocalizedText(
                    key: "app.wayfinder.sidebar.sources_monitored",
                    arguments: ["count": String(detectedManagerCount)]
                ),
                "checkmark.shield"
            )
        case let .updatesReady(count):
            return (
                WayfinderLocalizedText(key: "app.popover.wayfinder.context.plan_ready"),
                WayfinderLocalizedText(
                    key: "app.popover.wayfinder.context.updates_in_plan",
                    arguments: ["count": String(count)]
                ),
                "list.bullet.clipboard"
            )
        case .activeWork:
            return (
                WayfinderLocalizedText(key: "app.popover.wayfinder.context.work_in_progress"),
                projection.explanation,
                "arrow.triangle.2.circlepath"
            )
        case .approvalRequired:
            return (
                WayfinderLocalizedText(key: "app.popover.wayfinder.context.decision_required"),
                projection.explanation,
                "hand.raised.fill"
            )
        case .failedOrInterrupted:
            return (
                WayfinderLocalizedText(key: "app.popover.wayfinder.context.recovery_available"),
                projection.explanation,
                "arrow.uturn.backward.circle"
            )
        case .actionableFinding:
            return (
                findingContext?.title
                    ?? WayfinderLocalizedText(
                        key: "app.popover.wayfinder.context.environment_needs_review"
                    ),
                findingContext?.detail ?? projection.explanation,
                "point.3.connected.trianglepath.dotted"
            )
        case .offline:
            return (
                WayfinderLocalizedText(key: "app.popover.wayfinder.context.local_views_available"),
                WayfinderLocalizedText(key: "app.popover.wayfinder.context.local_views_detail"),
                "internaldrive"
            )
        case .refreshing:
            return (
                WayfinderLocalizedText(key: "app.popover.wayfinder.context.checking_environment"),
                projection.explanation,
                "arrow.clockwise"
            )
        case .serviceUnavailable:
            return (
                WayfinderLocalizedText(key: "app.popover.wayfinder.context.last_known_state"),
                projection.explanation,
                "bolt.horizontal.circle"
            )
        }
    }
}

struct WayfinderProjectionInput: Equatable {
    var serviceAvailable = true
    var networkAvailable = true
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
        } else if !input.networkAvailable {
            base = ProjectionBase(
                mode: .cachedPartialOffline,
                condition: .offline,
                titleKey: "app.wayfinder.course.offline.title",
                explanationKey: "app.wayfinder.course.offline.explanation",
                actionTitleKey: "app.wayfinder.action.open_dashboard",
                destination: .dashboard,
                entityID: nil
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
                focus: base.condition == .serviceUnavailable || base.condition == .offline
                    ? .serviceHealth
                    : (base.entityID == nil ? .primaryContent : .selectedEntity),
                originatingCondition: base.condition.kind
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

enum WayfinderPopoverFixtureName: String, Codable, CaseIterable, Hashable {
    case healthy
    case updatesReady = "updates-ready"
    case running
    case needsReview = "needs-review"
    case error
    case offline
}

struct WayfinderPopoverPresentationFixture: Equatable {
    static let currentSchemaVersion = "1.0.0"

    let schemaVersion: String
    let name: WayfinderPopoverFixtureName
    let presentation: WayfinderPopoverPresentation
}

enum WayfinderPopoverFixtureProvider {
    static let environmentKey = "HELM_WAYFINDER_POPOVER_FIXTURE"

    static func isActive(
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) -> Bool {
        selectedName(environment: environment) != nil
    }

    static func active(
        environment: [String: String] = ProcessInfo.processInfo.environment,
        now: Date = Date()
    ) -> WayfinderPopoverPresentationFixture? {
        guard let name = selectedName(environment: environment) else { return nil }
        return fixture(named: name, now: now)
    }

    static func fixture(
        named name: WayfinderPopoverFixtureName,
        now: Date = Date()
    ) -> WayfinderPopoverPresentationFixture {
        let projectionInput: WayfinderProjectionInput
        var relatedRouteStages: [WayfinderPopoverRouteStage] = []
        var relatedManagerIDsByStage: [WayfinderPopoverRouteStage: String] = [:]
        var findingContext: WayfinderPopoverFindingContext?

        switch name {
        case .healthy:
            projectionInput = WayfinderProjectionInput(
                freshnessDate: now.addingTimeInterval(-120),
                coverage: WayfinderCoverage(completed: 17, total: 17)
            )
        case .updatesReady:
            projectionInput = WayfinderProjectionInput(
                updateCount: 4,
                freshnessDate: now.addingTimeInterval(-120)
            )
            relatedRouteStages = [.apps, .packages]
        case .running:
            projectionInput = WayfinderProjectionInput(
                activeTaskIDs: ["fixture-task-running"],
                activeProgress: WayfinderDeterminateProgress(completed: 5, total: 12),
                freshnessDate: now
            )
            relatedRouteStages = [.apps]
        case .needsReview:
            projectionInput = WayfinderProjectionInput(
                actionableFindingIDs: ["mise"],
                freshnessDate: now.addingTimeInterval(-120)
            )
            relatedRouteStages = [.tools]
            relatedManagerIDsByStage = [.tools: "mise"]
            findingContext = WayfinderPopoverFindingContext(
                title: WayfinderLocalizedText(
                    key: "app.popover.wayfinder.context.manager_needs_decision",
                    arguments: ["manager": "mise"]
                ),
                detail: WayfinderLocalizedText(
                    key: "app.inspector.multi_instance.attention_title"
                )
            )
        case .error:
            projectionInput = WayfinderProjectionInput(
                failedTaskIDs: ["fixture-task-error"],
                freshnessDate: now.addingTimeInterval(-30)
            )
            relatedRouteStages = [.apps]
        case .offline:
            projectionInput = WayfinderProjectionInput(
                networkAvailable: false,
                freshnessDate: now.addingTimeInterval(-1_080)
            )
        }

        let presentation = WayfinderPopoverPresentationProjector.content(
            for: WayfinderPopoverPresentationInput(
                projection: WayfinderProjectionProjector.content(for: projectionInput),
                relatedRouteStages: relatedRouteStages,
                relatedManagerIDsByStage: relatedManagerIDsByStage,
                detectedManagerCount: 17,
                findingContext: findingContext
            )
        )

        return WayfinderPopoverPresentationFixture(
            schemaVersion: WayfinderPopoverPresentationFixture.currentSchemaVersion,
            name: name,
            presentation: presentation
        )
    }

    private static func selectedName(
        environment: [String: String]
    ) -> WayfinderPopoverFixtureName? {
        #if DEBUG
        guard let rawName = environment[environmentKey]?
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased() else {
            return nil
        }
        return WayfinderPopoverFixtureName(rawValue: rawName)
        #else
        return nil
        #endif
    }
}
