import SwiftUI

enum ControlCenterSection: String, CaseIterable, Identifiable {
    case overview
    case updates
    case packages
    case tasks
    case managers
    case settings

    var id: String { rawValue }

    var title: String {
        switch self {
        case .overview:
            return L10n.App.Navigation.dashboard.localized
        case .updates:
            return L10n.App.Section.updates.localized
        case .packages:
            return L10n.App.Navigation.packages.localized
        case .tasks:
            return L10n.App.Section.tasks.localized
        case .managers:
            return L10n.App.Navigation.managers.localized
        case .settings:
            return L10n.App.Settings.Tab.title.localized
        }
    }

    var icon: String {
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

final class ControlCenterContext: ObservableObject {
    @Published var selectedSection: ControlCenterSection? = .overview
    @Published var selectedManagerId: String?
    @Published var selectedPackageId: String?
    @Published var selectedTaskId: String?
    @Published var selectedUpgradePlanStepId: String?
    @Published var searchQuery: String = ""
    @Published var managerFilterId: String?
    @Published var showUpgradeSheet: Bool = false
    @Published var popoverOverlayRequest: PopoverOverlayRoute?
    @Published var popoverOverlayDismissToken: Int = 0
    @Published var popoverSearchFocusToken: Int = 0
    @Published var controlCenterSearchFocusToken: Int = 0
    @Published var isPopoverOverlayVisible: Bool = false
    @Published var suppressWindowBackgroundDragging: Bool = false

    func clearInspectorSelection() {
        selectedManagerId = nil
        selectedPackageId = nil
        selectedTaskId = nil
        selectedUpgradePlanStepId = nil
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
}

enum PopoverOverlayRoute: String, Identifiable {
    case search
    case quickSettings
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
