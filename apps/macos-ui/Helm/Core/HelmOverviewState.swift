import Combine

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
    @Published private(set) var outdatedCountByManager: [String: Int] = [:]
    @Published private(set) var managerHealthById: [String: OperationalHealth] = [:]
    @Published private(set) var recentTasksTop10: [TaskItem] = []
    @Published private(set) var runningTasksTop4: [TaskItem] = []
    @Published private(set) var popoverManagerRows: [ManagerInfo] = []

    func apply(
        wayfinderInput: WayfinderProjectionInput,
        environmentBriefInput: EnvironmentBriefProjectionInput,
        failedTaskCount: Int,
        runningTaskCount: Int,
        outdatedPackagesCount: Int,
        isRefreshing: Bool,
        visibleManagers: [ManagerInfo],
        outdatedCountByManager: [String: Int],
        managerHealthById: [String: OperationalHealth],
        recentTasksTop10: [TaskItem],
        runningTasksTop4: [TaskItem],
        popoverManagerRows: [ManagerInfo]
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
        self.outdatedCountByManager = outdatedCountByManager
        self.managerHealthById = managerHealthById
        self.recentTasksTop10 = recentTasksTop10
        self.runningTasksTop4 = runningTasksTop4
        self.popoverManagerRows = popoverManagerRows
    }

    private static func operationalHealth(
        for condition: WayfinderCondition
    ) -> OperationalHealth {
        switch condition {
        case .healthy, .updatesReady:
            return .healthy
        case .activeWork, .refreshing:
            return .running
        case .failedOrInterrupted:
            return .error
        case .approvalRequired, .actionableFinding, .serviceUnavailable:
            return .attention
        }
    }
}
