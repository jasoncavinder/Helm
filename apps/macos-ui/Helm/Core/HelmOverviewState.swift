import Combine

struct WayfinderPopoverDerivedState: Equatable {
    let relatedRouteStages: [WayfinderPopoverRouteStage]
    let relatedManagerIDsByStage: [WayfinderPopoverRouteStage: String]
    let detectedManagerCount: Int
    let findingContext: WayfinderPopoverFindingContext?
}

final class HelmOverviewState: ObservableObject {
    private let environmentBriefFixture = EnvironmentBriefFixtureProvider.active()?.brief

    @Published private(set) var wayfinderProjection: WayfinderPresentationProjection = .initial
    @Published private(set) var researchAmbientHealthPresentation: WayfinderPopoverPresentation?
    @Published private(set) var environmentBrief: EnvironmentBrief?
    @Published private(set) var aggregateHealth: OperationalHealth = .healthy
    @Published private(set) var failedTaskCount: Int = 0
    @Published private(set) var runningTaskCount: Int = 0
    @Published private(set) var outdatedPackagesCount: Int = 0
    @Published private(set) var isRefreshing: Bool = false
    @Published private(set) var visibleManagers: [ManagerInfo] = []
    @Published private(set) var wayfinderRelatedRouteStages: [WayfinderPopoverRouteStage] = []
    @Published private(set) var wayfinderRelatedManagerIDsByStage: [
        WayfinderPopoverRouteStage: String
    ] = [:]
    @Published private(set) var detectedManagerCount: Int = 0
    @Published private(set) var wayfinderFindingContext: WayfinderPopoverFindingContext?
    @Published private(set) var outdatedCountByManager: [String: Int] = [:]
    @Published private(set) var managerHealthById: [String: OperationalHealth] = [:]
    @Published private(set) var recentTasksTop10: [TaskItem] = []

    func apply(
        wayfinderInput: WayfinderProjectionInput,
        environmentBriefInput: EnvironmentBriefProjectionInput,
        failedTaskCount: Int,
        runningTaskCount: Int,
        outdatedPackagesCount: Int,
        isRefreshing: Bool,
        visibleManagers: [ManagerInfo],
        wayfinderPopoverState: WayfinderPopoverDerivedState,
        outdatedCountByManager: [String: Int],
        managerHealthById: [String: OperationalHealth],
        recentTasksTop10: [TaskItem]
    ) {
        guard researchAmbientHealthPresentation == nil else { return }
        let nextProjection = WayfinderProjectionProjector.project(
            wayfinderInput,
            replacing: wayfinderProjection
        )
        if nextProjection != wayfinderProjection {
            wayfinderProjection = nextProjection
        }
        let nextEnvironmentBrief = environmentBriefFixture
            ?? EnvironmentBriefProjector.project(
                environmentBriefInput,
                replacing: environmentBrief
            )
        if nextEnvironmentBrief != environmentBrief {
            environmentBrief = nextEnvironmentBrief
        }
        aggregateHealth = Self.operationalHealth(for: nextProjection.content.condition)
        self.failedTaskCount = failedTaskCount
        self.runningTaskCount = runningTaskCount
        self.outdatedPackagesCount = outdatedPackagesCount
        self.isRefreshing = isRefreshing
        self.visibleManagers = visibleManagers
        self.wayfinderRelatedRouteStages = wayfinderPopoverState.relatedRouteStages
        self.wayfinderRelatedManagerIDsByStage = wayfinderPopoverState.relatedManagerIDsByStage
        self.detectedManagerCount = wayfinderPopoverState.detectedManagerCount
        self.wayfinderFindingContext = wayfinderPopoverState.findingContext
        self.outdatedCountByManager = outdatedCountByManager
        self.managerHealthById = managerHealthById
        self.recentTasksTop10 = recentTasksTop10
    }

    func applyResearchAmbientHealthPresentation(
        _ presentation: WayfinderPopoverPresentation
    ) {
        researchAmbientHealthPresentation = presentation
        let content = presentation.projection
        if content != wayfinderProjection.content {
            wayfinderProjection = WayfinderPresentationProjection(
                revision: wayfinderProjection.revision &+ 1,
                content: content
            )
        }

        aggregateHealth = Self.operationalHealth(for: content.condition)
        if case let .failedOrInterrupted(failed, interrupted) = content.condition {
            failedTaskCount = failed + interrupted
        } else {
            failedTaskCount = 0
        }
        runningTaskCount = 0
        outdatedPackagesCount = 0
        isRefreshing = false

        wayfinderRelatedRouteStages = presentation.routeItems.map(\.stage)
        wayfinderRelatedManagerIDsByStage = presentation.routeItems.reduce(into: [:]) {
            if let managerID = $1.managerID {
                $0[$1.stage] = managerID
            }
        }
        detectedManagerCount = content.coverage?.completed ?? 0
        wayfinderFindingContext = WayfinderPopoverFindingContext(
            title: presentation.contextTitle,
            detail: presentation.contextDetail
        )
    }

    func applyResearchAmbientHealthRuntimeState(
        _ state: ResearchAmbientHealthRuntimeState?
    ) -> Bool {
        guard let state else { return false }
        applyResearchAmbientHealthPresentation(state.presentation)
        return state.serviceConnected
    }

    private static func operationalHealth(
        for condition: WayfinderCondition
    ) -> OperationalHealth {
        switch condition {
        case .healthy:
            return .healthy
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
        }
    }
}
