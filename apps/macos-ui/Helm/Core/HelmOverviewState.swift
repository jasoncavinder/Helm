import Combine

struct WayfinderPopoverDerivedState: Equatable {
    let relatedRouteStages: [WayfinderPopoverRouteStage]
    let detectedManagerCount: Int
    let findingContext: WayfinderPopoverFindingContext?
}

final class HelmOverviewState: ObservableObject {
    private let environmentBriefFixture = EnvironmentBriefFixtureProvider.active()?.brief

    @Published private(set) var wayfinderProjection: WayfinderPresentationProjection = .initial
    @Published private(set) var environmentBrief: EnvironmentBrief?
    @Published private(set) var aggregateHealth: OperationalHealth = .healthy
    @Published private(set) var failedTaskCount: Int = 0
    @Published private(set) var runningTaskCount: Int = 0
    @Published private(set) var outdatedPackagesCount: Int = 0
    @Published private(set) var isRefreshing: Bool = false
    @Published private(set) var visibleManagers: [ManagerInfo] = []
    @Published private(set) var wayfinderRelatedRouteStages: [WayfinderPopoverRouteStage] = []
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
        self.detectedManagerCount = wayfinderPopoverState.detectedManagerCount
        self.wayfinderFindingContext = wayfinderPopoverState.findingContext
        self.outdatedCountByManager = outdatedCountByManager
        self.managerHealthById = managerHealthById
        self.recentTasksTop10 = recentTasksTop10
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
