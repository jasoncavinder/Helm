import SwiftUI

enum ControlCenterSection: String, CaseIterable, Identifiable {
    case overview
    case updates
    case packages
    case managers
    case tasks

    var id: String { rawValue }

    static let wayfinderWorkspaces: [ControlCenterSection] = [
        .overview,
        .updates,
        .packages,
        .tasks
    ]

    var title: String {
        switch self {
        case .overview:
            return L10n.App.Navigation.dashboard.localized
        case .updates:
            return "app.wayfinder.destination.plan".localized
        case .packages:
            return "app.wayfinder.destination.library".localized
        case .tasks:
            return "app.wayfinder.destination.activity".localized
        case .managers:
            return "app.wayfinder.destination.environment".localized
        }
    }

    var icon: String {
        switch self {
        case .overview:
            return "gauge.with.dots.needle.33percent"
        case .updates:
            return "point.topleft.down.to.point.bottomright.curvepath"
        case .packages:
            return "square.grid.2x2"
        case .tasks:
            return "waveform.path.ecg"
        case .managers:
            return "point.3.connected.trianglepath.dotted"
        }
    }

    var supportsInspector: Bool {
        switch self {
        case .overview:
            return false
        case .updates, .packages, .managers, .tasks:
            return true
        }
    }
}

enum ManagerAuthority: CaseIterable {
    case authoritative
    case standard
    case guarded

    var key: String {
        switch self {
        case .authoritative:
            return L10n.App.Updates.Authority.authoritative
        case .standard:
            return L10n.App.Updates.Authority.standard
        case .guarded:
            return L10n.App.Updates.Authority.guarded
        }
    }
}

enum OperationalHealth: Equatable {
    case healthy
    case updatesReady
    case needsReview
    case error
    case running
    case unavailable
    case notInstalled

    var icon: String {
        switch self {
        case .healthy:
            return "checkmark.circle.fill"
        case .updatesReady:
            return "arrow.up.circle.fill"
        case .needsReview:
            return "exclamationmark.triangle.fill"
        case .error:
            return "xmark.octagon.fill"
        case .running:
            return "arrow.triangle.2.circlepath"
        case .unavailable, .notInstalled:
            return "minus.circle.fill"
        }
    }

    var color: Color {
        switch self {
        case .healthy:
            return HelmTheme.stateHealthy
        case .updatesReady:
            return HelmTheme.stateUpdatesReady
        case .needsReview:
            return HelmTheme.stateNeedsReview
        case .error:
            return HelmTheme.stateError
        case .running:
            return HelmTheme.stateRunning
        case .unavailable, .notInstalled:
            return HelmTheme.stateUnavailable
        }
    }

    var key: String {
        switch self {
        case .healthy:
            return L10n.App.Health.healthy
        case .updatesReady:
            return L10n.App.Health.updatesReady
        case .needsReview:
            return L10n.App.Health.needsReview
        case .error:
            return L10n.App.Health.error
        case .running:
            return L10n.App.Health.running
        case .unavailable:
            return L10n.App.Health.unavailable
        case .notInstalled:
            return L10n.App.Health.notInstalled
        }
    }
}

extension WayfinderLocalizedText {
    var localized: String {
        key.localized(with: arguments)
    }
}

extension WayfinderDestination {
    var legacyControlCenterSection: ControlCenterSection {
        switch self {
        case .dashboard:
            return .overview
        case .plan:
            return .updates
        case .library:
            return .packages
        case .activity:
            return .tasks
        case .environment:
            return .managers
        }
    }
}

final class ControlCenterContext: ObservableObject {
    @Published var selectedSection: ControlCenterSection? = .overview
    @Published var selectedManagerId: String?
    @Published var selectedPackageId: String?
    @Published var selectedTaskId: String?
    @Published var selectedUpgradePlanStepId: String?
    @Published var searchQuery: String = ""
    @Published var isControlCenterSearchPresented: Bool = false
    @Published var planManagerScopeId: String = HelmCore.allManagersScopeId
    @Published var planPackageFilter: String = ""
    @Published var managerFilterId: String?
    @Published private var environmentRouteFilterState = WayfinderEnvironmentRouteFilterState()
    @Published private var upgradeSheetPresentation = UpgradeSheetPresentationState()
    let controlCenterSearchFocusRouter = ControlCenterSearchFocusRouter()
    let settingsOpenRouter = HelmSettingsOpenRouter()
    @Published var isSidebarVisible: Bool = true
    @Published var isInspectorVisible: Bool = true
    @Published var managerInstallSheetRequestManagerId: String?
    @Published var managerInstallSheetRequestToken: Int = 0
    @Published private var firstRunSession = EnvironmentBriefFirstRunSession()
    @Published private(set) var dashboardFocusRequestToken: Int = 0
    @Published private(set) var wayfinderNavigationState = WayfinderNavigationState()
    private var pendingDashboardFocusTarget: WayfinderFocusTarget?

    var showUpgradeSheet: Bool {
        upgradeSheetPresentation.isPresented
    }

    var upgradeSheetHost: UpgradeSheetHost {
        upgradeSheetPresentation.host
    }

    var upgradeSheetIntent: UpgradeSheetIntent {
        upgradeSheetPresentation.intent
    }

    var environmentRouteStage: WayfinderPopoverRouteStage? {
        environmentRouteFilterState.stage
    }

    func presentUpgradeSheet(in host: UpgradeSheetHost) {
        upgradeSheetPresentation.presentUpgradeAll(in: host)
    }

    func presentReviewedUpgradePlanSheet(
        in host: UpgradeSheetHost,
        managerScopeID: String,
        packageFilter: String,
        selectedStepIDs: Set<String>
    ) {
        upgradeSheetPresentation.presentReviewedPlan(
            in: host,
            managerScopeID: managerScopeID,
            packageFilter: packageFilter,
            selectedStepIDs: selectedStepIDs
        )
    }

    func dismissUpgradeSheet() {
        upgradeSheetPresentation.dismiss()
    }

    func clearInspectorSelection() {
        clearInspectorSelection(except: nil)
    }

    func select(_ section: ControlCenterSection) {
        environmentRouteFilterState.clear()
        selectedSection = section
    }

    func clearEnvironmentRouteStage() {
        environmentRouteFilterState.clear()
    }

    func toggleSidebar() {
        isSidebarVisible.toggle()
    }

    func toggleInspector() {
        guard (selectedSection ?? .overview).supportsInspector else { return }
        isInspectorVisible.toggle()
    }

    func navigate(to deepLink: WayfinderDeepLink) {
        wayfinderNavigationState.record(deepLink)
        clearInspectorSelection()

        selectedSection = deepLink.destination.legacyControlCenterSection
        environmentRouteFilterState.apply(deepLink)

        if deepLink.destination == .dashboard, deepLink.focus == .serviceHealth {
            pendingDashboardFocusTarget = .serviceHealth
            dashboardFocusRequestToken &+= 1
        }

        guard let entityID = deepLink.entityID else { return }
        switch deepLink.destination {
        case .dashboard:
            break
        case .plan:
            selectedUpgradePlanStepId = entityID
        case .library:
            selectedPackageId = entityID
        case .activity:
            selectedTaskId = entityID
        case .environment:
            selectedManagerId = entityID
        }
    }

    func takeDashboardFocusRequest() -> WayfinderFocusTarget? {
        defer { pendingDashboardFocusTarget = nil }
        return pendingDashboardFocusTarget
    }

    func alignInspectorSelection(for section: ControlCenterSection?) {
        clearInspectorSelection(except: section)
    }

    func requestManagerInstallSheet(for managerId: String) {
        managerInstallSheetRequestManagerId = managerId
        managerInstallSheetRequestToken += 1
    }

    func shouldPresentFirstRun(
        mode: EnvironmentBriefFirstRunMode,
        hasCompletedOnboarding: Bool
    ) -> Bool {
        firstRunSession.shouldPresent(
            mode: mode,
            hasCompletedOnboarding: hasCompletedOnboarding
        )
    }

    func dismissFirstRunPreview() {
        firstRunSession.dismissPreview()
    }

    private func clearInspectorSelection(except section: ControlCenterSection?) {
        if section != .managers, selectedManagerId != nil {
            selectedManagerId = nil
        }
        if section != .packages, selectedPackageId != nil {
            selectedPackageId = nil
        }
        if section != .tasks, selectedTaskId != nil {
            selectedTaskId = nil
        }
        if section != .updates, selectedUpgradePlanStepId != nil {
            selectedUpgradePlanStepId = nil
        }
    }
}

struct HealthBadgeView: View {
    let status: OperationalHealth

    var body: some View {
        Label(status.key.localized, systemImage: status.icon)
            .font(.caption.weight(.semibold))
            .lineLimit(1)
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .foregroundColor(status.color)
            .background(
                Capsule()
                    .fill(status.color.opacity(0.15))
            )
            .accessibilityLabel(status.key.localized)
    }
}

struct WayfinderFooterStatusBadge: View {
    let status: WayfinderFooterStatus

    private var color: Color {
        switch status {
        case .healthy:
            return HelmTheme.stateHealthy
        case .updatesReady:
            return HelmTheme.stateUpdatesReady
        case .running:
            return HelmTheme.stateRunning
        case .needsReview:
            return HelmTheme.stateNeedsReview
        case .error:
            return HelmTheme.stateError
        case .unavailable:
            return HelmTheme.stateUnavailable
        }
    }

    var body: some View {
        Label(status.titleKey.localized, systemImage: status.icon)
            .font(.caption.weight(.semibold))
            .lineLimit(1)
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .foregroundColor(color)
            .background(
                Capsule()
                    .fill(color.opacity(0.15))
            )
            .contentShape(Capsule())
            .accessibilityLabel(status.titleKey.localized)
    }
}

func authority(for managerId: String) -> ManagerAuthority {
    guard let manager = ManagerInfo.all.first(where: { $0.id == managerId }) else {
        return .standard
    }
    return manager.authority
}
