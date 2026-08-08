import SwiftUI

enum ControlCenterSection: String, CaseIterable, Identifiable {
    case overview
    case updates
    case packages
    case managers
    case tasks
    case settings

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
        case .settings:
            return L10n.App.Settings.Tab.title.localized
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
        case .settings:
            return "gearshape"
        }
    }

    var supportsInspector: Bool {
        switch self {
        case .overview, .settings:
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

enum OperationalHealth {
    case healthy
    case attention
    case error
    case running
    case notInstalled

    var icon: String {
        switch self {
        case .healthy:
            return "checkmark.circle.fill"
        case .attention:
            return "exclamationmark.triangle.fill"
        case .error:
            return "xmark.octagon.fill"
        case .running:
            return "arrow.triangle.2.circlepath"
        case .notInstalled:
            return "minus.circle.fill"
        }
    }

    var color: Color {
        switch self {
        case .healthy:
            return .green
        case .attention:
            return .orange
        case .error:
            return .red
        case .running:
            return .blue
        case .notInstalled:
            return .gray
        }
    }

    var key: String {
        switch self {
        case .healthy:
            return L10n.App.Health.healthy
        case .attention:
            return L10n.App.Health.attention
        case .error:
            return L10n.App.Health.error
        case .running:
            return L10n.App.Health.running
        case .notInstalled:
            return L10n.App.Health.notInstalled
        }
    }
}

enum UpgradeSheetHost {
    case popover
    case controlCenter
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
    @Published var managerFilterId: String?
    @Published var showUpgradeSheet: Bool = false
    @Published var upgradeSheetHost: UpgradeSheetHost = .controlCenter
    @Published var popoverOverlayRequest: PopoverOverlayRoute?
    @Published var popoverOverlayDismissToken: Int = 0
    @Published var popoverSearchFocusToken: Int = 0
    @Published var controlCenterSearchFocusToken: Int = 0
    @Published var isPopoverOverlayVisible: Bool = false
    @Published var suppressWindowBackgroundDragging: Bool = false
    @Published var isSidebarVisible: Bool = true
    @Published var isInspectorVisible: Bool = true
    @Published var managerInstallSheetRequestManagerId: String?
    @Published var managerInstallSheetRequestToken: Int = 0

    func presentUpgradeSheet(in host: UpgradeSheetHost) {
        upgradeSheetHost = host
        showUpgradeSheet = true
    }

    func dismissUpgradeSheet() {
        showUpgradeSheet = false
    }

    func clearInspectorSelection() {
        selectedManagerId = nil
        selectedPackageId = nil
        selectedTaskId = nil
        selectedUpgradePlanStepId = nil
    }

    func select(_ section: ControlCenterSection) {
        selectedSection = section
    }

    func toggleSidebar() {
        isSidebarVisible.toggle()
    }

    func toggleInspector() {
        guard (selectedSection ?? .overview).supportsInspector else { return }
        isInspectorVisible.toggle()
    }

    func navigate(to deepLink: WayfinderDeepLink) {
        clearInspectorSelection()

        // Remove this compatibility route when service health moves into the native Dashboard.
        if deepLink.destination == .dashboard, deepLink.focus == .serviceHealth {
            selectedSection = .settings
            return
        }

        selectedSection = deepLink.destination.legacyControlCenterSection

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

    func alignInspectorSelection(for section: ControlCenterSection?) {
        guard let section else {
            clearInspectorSelection()
            return
        }

        switch section {
        case .overview, .settings:
            clearInspectorSelection()
        case .updates:
            let retainedStepId = selectedUpgradePlanStepId
            clearInspectorSelection()
            selectedUpgradePlanStepId = retainedStepId
        case .packages:
            let retainedPackageId = selectedPackageId
            clearInspectorSelection()
            selectedPackageId = retainedPackageId
        case .tasks:
            let retainedTaskId = selectedTaskId
            clearInspectorSelection()
            selectedTaskId = retainedTaskId
        case .managers:
            let retainedManagerId = selectedManagerId
            clearInspectorSelection()
            selectedManagerId = retainedManagerId
        }
    }

    func requestManagerInstallSheet(for managerId: String) {
        managerInstallSheetRequestManagerId = managerId
        managerInstallSheetRequestToken += 1
    }
}

enum PopoverOverlayRoute: String, Identifiable {
    case search
    case about
    case confirmQuit

    var id: String { rawValue }
}

struct HealthBadgeView: View {
    let status: OperationalHealth

    var body: some View {
        Label(status.key.localized, systemImage: status.icon)
            .font(.caption.weight(.semibold))
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

func authority(for managerId: String) -> ManagerAuthority {
    guard let manager = ManagerInfo.all.first(where: { $0.id == managerId }) else {
        return .standard
    }
    return manager.authority
}
