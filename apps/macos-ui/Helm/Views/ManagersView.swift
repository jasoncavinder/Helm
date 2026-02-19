import SwiftUI

struct ManagersSectionView: View {
    @ObservedObject private var core = HelmCore.shared
    @EnvironmentObject private var context: ControlCenterContext

    private var groupedManagers: [(authority: ManagerAuthority, managers: [ManagerInfo])] {
        ManagerAuthority.allCases.map { authorityLevel in
            let managers = ManagerInfo.implemented
                .filter { $0.authority == authorityLevel }
                .sorted { localizedManagerDisplayName($0.id).localizedCaseInsensitiveCompare(localizedManagerDisplayName($1.id)) == .orderedAscending }
            return (authority: authorityLevel, managers: managers)
        }
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                Text(ControlCenterSection.managers.title)
                    .font(.title2.weight(.semibold))
                    .padding(.horizontal, 20)
                    .padding(.top, 20)

                ForEach(groupedManagers, id: \.authority) { group in
                    if !group.managers.isEmpty {
                        VStack(alignment: .leading, spacing: 8) {
                            Text(group.authority.key.localized)
                                .font(.caption.weight(.semibold))
                                .foregroundStyle(.secondary)
                                .textCase(.uppercase)
                                .padding(.horizontal, 20)

                            ForEach(group.managers) { manager in
                                ManagerSectionRow(
                                    manager: manager,
                                    status: core.managerStatuses[manager.id],
                                    outdatedCount: core.outdatedCount(forManagerId: manager.id),
                                    packageCount: core.installedPackages.filter { $0.managerId == manager.id }.count,
                                    operationStatus: core.managerOperations[manager.id],
                                    onSelect: {
                                        context.selectedManagerId = manager.id
                                    },
                                    onViewPackages: {
                                        context.selectedManagerId = manager.id
                                        context.managerFilterId = manager.id
                                        context.selectedSection = .packages
                                    }
                                )
                            }
                        }
                    }
                }

                if ManagerInfo.implemented.isEmpty {
                    Text(L10n.App.ManagersSection.empty.localized)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .padding(.horizontal, 20)
                }
            }
            .padding(.bottom, 18)
        }
    }
}

private struct ManagerSectionRow: View {
    @ObservedObject private var core = HelmCore.shared

    let manager: ManagerInfo
    let status: ManagerStatus?
    let outdatedCount: Int
    let packageCount: Int
    let operationStatus: String?
    let onSelect: () -> Void
    let onViewPackages: () -> Void

    @State private var confirmAction: ConfirmAction?

    private enum ConfirmAction: Identifiable {
        case install
        case update
        case uninstall

        var id: String {
            switch self {
            case .install:
                return "install"
            case .update:
                return "update"
            case .uninstall:
                return "uninstall"
            }
        }
    }

    private var detected: Bool {
        status?.detected ?? false
    }

    private var enabled: Bool {
        status?.enabled ?? true
    }

    private var currentHealth: OperationalHealth {
        core.health(forManagerId: manager.id)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 10) {
                HealthBadgeView(status: currentHealth)

                VStack(alignment: .leading, spacing: 3) {
                    Text(localizedManagerDisplayName(manager.id))
                        .font(.body.weight(.medium))
                    HStack(spacing: 6) {
                        Text(L10n.App.Managers.Label.packageCount.localized(with: ["count": packageCount]))
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        Text("|")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        Text(L10n.App.Managers.Tooltip.outdated.localized(with: ["count": outdatedCount]))
                            .font(.caption)
                            .foregroundStyle(outdatedCount == 0 ? Color.secondary : Color.orange)
                    }
                }

                Spacer()

                if let operationStatus {
                    HStack(spacing: 4) {
                        ProgressView()
                            .controlSize(.mini)
                        Text(operationStatus)
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                    }
                } else {
                    Text(detected ? (enabled ? L10n.App.Managers.State.enabled.localized : L10n.App.Managers.State.disabled.localized) : L10n.App.Managers.State.notInstalled.localized)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                if detected {
                    Toggle("", isOn: Binding(
                        get: { enabled },
                        set: { _ in
                            core.setManagerEnabled(manager.id, enabled: !enabled)
                        }
                    ))
                    .toggleStyle(.switch)
                    .labelsHidden()
                    .scaleEffect(0.75)
                }
            }

            HStack(spacing: 8) {
                if manager.canInstall && !detected {
                    Button(L10n.Common.install.localized) {
                        confirmAction = .install
                    }
                    .helmPointer()
                }
                if manager.canUpdate && detected {
                    Button(L10n.Common.update.localized) {
                        confirmAction = .update
                    }
                    .helmPointer()
                }
                if manager.canUninstall && detected {
                    Button(L10n.Common.uninstall.localized) {
                        confirmAction = .uninstall
                    }
                    .helmPointer()
                }

                Spacer()

                if outdatedCount > 0 {
                    Button(L10n.App.Settings.Action.upgradeAll.localized) {
                        core.upgradeAllPackages(forManagerId: manager.id)
                    }
                    .helmPointer()
                }

                Button(L10n.App.Managers.Action.viewPackages.localized) {
                    onViewPackages()
                }
                .disabled(packageCount == 0)
                .helmPointer(enabled: packageCount > 0)
            }
            .font(.caption)
        }
        .padding(12)
        .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .padding(.horizontal, 20)
        .contentShape(Rectangle())
        .onTapGesture {
            onSelect()
        }
        .helmPointer()
        .accessibilityElement(children: .contain)
        .accessibilityLabel(localizedManagerDisplayName(manager.id))
        .accessibilityValue([
            currentHealth.key.localized,
            detected ? (enabled ? L10n.App.Managers.State.enabled.localized : L10n.App.Managers.State.disabled.localized) : L10n.App.Managers.State.notInstalled.localized,
            L10n.App.Managers.Label.packageCount.localized(with: ["count": packageCount])
        ].joined(separator: ", "))
        .alert(item: $confirmAction) { action in
            switch action {
            case .install:
                return Alert(
                    title: Text(L10n.App.Managers.Alert.installTitle.localized(with: ["manager": localizedManagerDisplayName(manager.id)])),
                    message: Text(L10n.App.Managers.Alert.installMessage.localized(with: ["manager_short": manager.shortName])),
                    primaryButton: .default(Text(L10n.Common.install.localized)) { core.installManager(manager.id) },
                    secondaryButton: .cancel()
                )
            case .update:
                return Alert(
                    title: Text(L10n.App.Managers.Alert.updateTitle.localized(with: ["manager": localizedManagerDisplayName(manager.id)])),
                    message: Text(L10n.App.Managers.Alert.updateMessage.localized),
                    primaryButton: .default(Text(L10n.Common.update.localized)) { core.updateManager(manager.id) },
                    secondaryButton: .cancel()
                )
            case .uninstall:
                return Alert(
                    title: Text(L10n.App.Managers.Alert.uninstallTitle.localized(with: ["manager": localizedManagerDisplayName(manager.id)])),
                    message: Text(L10n.App.Managers.Alert.uninstallMessage.localized(with: ["manager_short": manager.shortName])),
                    primaryButton: .destructive(Text(L10n.Common.uninstall.localized)) { core.uninstallManager(manager.id) },
                    secondaryButton: .cancel()
                )
            }
        }
    }
}

// Backward compatibility wrapper for legacy references.
struct ManagersView: View {
    @Binding var selectedTab: HelmTab

    var body: some View {
        ManagersSectionView()
    }
}
