import SwiftUI
import UniformTypeIdentifiers

struct ManagersSectionView: View {
    private let core = HelmCore.shared
    @ObservedObject private var managersState = HelmCore.shared.managersState
    @EnvironmentObject private var context: ControlCenterContext
    @State private var draggedManagerId: String?

    private var groupedManagers: [(authority: ManagerAuthority, managers: [ManagerInfo])] {
        [
            (authority: .authoritative, managers: managersState.authoritativeManagers),
            (authority: .standard, managers: managersState.standardManagers),
            (authority: .guarded, managers: managersState.guardedManagers)
        ]
    }

    private var hasImplementedManagers: Bool {
        !managersState.authoritativeManagers.isEmpty
            || !managersState.standardManagers.isEmpty
            || !managersState.guardedManagers.isEmpty
    }

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 12) {
                Text(ControlCenterSection.managers.title)
                    .font(.title2.weight(.semibold))
                    .padding(.horizontal, 20)
                    .padding(.top, 20)

                ForEach(groupedManagers, id: \.authority) { group in
                    if !group.managers.isEmpty {
                        VStack(alignment: .leading, spacing: 8) {
                            Text(group.authority.key.localized)
                                .font(.caption.weight(.semibold))
                                .foregroundColor(.secondary)
                                .textCase(.uppercase)
                                .padding(.horizontal, 20)

                            ForEach(group.managers) { manager in
                                ManagerSectionRow(
                                    manager: manager,
                                    status: managersState.managerStatusesById[manager.id],
                                    health: core.health(forManagerId: manager.id),
                                    outdatedCount: managersState.outdatedCountByManager[manager.id, default: 0],
                                    packageCount: managersState.installedCountByManager[manager.id, default: 0],
                                    operationStatus: managersState.managerOperationsById[manager.id],
                                    isManagerUninstalling: core.isManagerUninstalling(manager.id),
                                    isSelected: context.selectedManagerId == manager.id,
                                    onSelect: {
                                        context.selectedManagerId = manager.id
                                        context.selectedPackageId = nil
                                        context.selectedTaskId = nil
                                        context.selectedUpgradePlanStepId = nil
                                    }
                                )
                                .onDrag {
                                    draggedManagerId = manager.id
                                    context.suppressWindowBackgroundDragging = true
                                    return NSItemProvider(object: manager.id as NSString)
                                }
                                .onDrop(
                                    of: [UTType.text.identifier],
                                    delegate: ManagerPriorityDropDelegate(
                                        core: core,
                                        authority: group.authority,
                                        targetManagerId: manager.id,
                                        draggedManagerId: $draggedManagerId,
                                        suppressWindowBackgroundDragging: $context.suppressWindowBackgroundDragging
                                    )
                                )
                            }
                        }
                    }
                }

                if !hasImplementedManagers {
                    Text(L10n.App.ManagersSection.empty.localized)
                        .font(.subheadline)
                        .foregroundColor(.secondary)
                        .padding(.horizontal, 20)
                }
            }
            .padding(.bottom, 18)
        }
        .onHover { hovering in
            context.suppressWindowBackgroundDragging = hovering || draggedManagerId != nil
        }
        .onDisappear {
            draggedManagerId = nil
            context.suppressWindowBackgroundDragging = false
        }
    }
}

private struct ManagerSectionRow: View {
    private let core = HelmCore.shared

    let manager: ManagerInfo
    let status: ManagerStatus?
    let health: OperationalHealth
    let outdatedCount: Int
    let packageCount: Int
    let operationStatus: String?
    let isManagerUninstalling: Bool
    let isSelected: Bool
    let onSelect: () -> Void

    private var detected: Bool {
        core.isManagerDetected(manager.id)
    }

    private var enabled: Bool {
        status?.enabled ?? true
    }

    private var isEligibleForEnablement: Bool {
        status?.isEligible ?? true
    }

    private var ineligibleReason: String? {
        guard detected, !isEligibleForEnablement else { return nil }
        if let key = status?.ineligibleServiceErrorKey?.trimmingCharacters(in: .whitespacesAndNewlines),
           !key.isEmpty
        {
            return key.localized
        }
        if let message = status?.ineligibleReasonMessage?.trimmingCharacters(in: .whitespacesAndNewlines),
           !message.isEmpty
        {
            return message
        }
        return nil
    }

    private var enableToggleDisabled: Bool {
        ineligibleReason != nil && !enabled
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 10) {
                Image(systemName: "line.3.horizontal")
                    .font(.caption.weight(.semibold))
                    .foregroundColor(HelmTheme.textSecondary)
                    .frame(width: 12)
                    .accessibilityHidden(true)

                HealthBadgeView(status: health)

                VStack(alignment: .leading, spacing: 3) {
                    Text(localizedManagerDisplayName(manager.id))
                        .font(.body.weight(.medium))
                    HStack(spacing: 6) {
                        Text(L10n.App.Managers.Label.packageCount.localized(with: ["count": packageCount]))
                            .font(.caption)
                            .foregroundColor(.secondary)
                        Text("|")
                            .font(.caption)
                            .foregroundColor(.secondary)
                        Text(L10n.App.Managers.Tooltip.outdated.localized(with: ["count": outdatedCount]))
                            .font(.caption)
                            .foregroundColor(outdatedCount == 0 ? HelmTheme.textSecondary : HelmTheme.stateAttention)
                    }
                }

                Spacer()

                if let operationStatus {
                    HStack(spacing: 4) {
                        ProgressView()
                            .controlSize(.mini)
                        Text(operationStatus)
                            .font(.caption2)
                            .foregroundColor(.secondary)
                    }
                } else {
                    Text(detected ? (enabled ? L10n.App.Managers.State.enabled.localized : L10n.App.Managers.State.disabled.localized) : L10n.App.Managers.State.notInstalled.localized)
                        .font(.caption)
                        .foregroundColor(.secondary)
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
                    .disabled(enableToggleDisabled || isManagerUninstalling)
                }
            }

            HStack(spacing: 8) {
                Spacer()

                if enabled && outdatedCount > 0 {
                    Button(L10n.App.Settings.Action.upgradeAll.localized) {
                        core.upgradeAllPackages(forManagerId: manager.id)
                    }
                    .disabled(isManagerUninstalling)
                    .helmPointer(enabled: !isManagerUninstalling)
                }
            }
            .font(.caption)

            if let ineligibleReason {
                Text(ineligibleReason)
                    .font(.caption2)
                    .foregroundColor(.secondary)
            }
        }
        .padding(12)
        .helmCardSurface(cornerRadius: 12)
        .overlay(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .fill(isSelected ? HelmTheme.selectionFill : Color.clear)
                .overlay(
                    RoundedRectangle(cornerRadius: 12, style: .continuous)
                        .strokeBorder(isSelected ? HelmTheme.selectionStroke : Color.clear, lineWidth: 0.9)
                )
                .allowsHitTesting(false)
        )
        .padding(.horizontal, 20)
        .contentShape(Rectangle())
        .onTapGesture {
            onSelect()
        }
        .helmPointer()
        .accessibilityElement(children: .contain)
        .accessibilityLabel(localizedManagerDisplayName(manager.id))
        .accessibilityValue([
            health.key.localized,
            detected ? (enabled ? L10n.App.Managers.State.enabled.localized : L10n.App.Managers.State.disabled.localized) : L10n.App.Managers.State.notInstalled.localized,
            L10n.App.Managers.Label.packageCount.localized(with: ["count": packageCount])
        ].joined(separator: ", "))
    }

}

// Backward compatibility wrapper for legacy references.
struct ManagersView: View {
    @Binding var selectedTab: HelmTab

    var body: some View {
        ManagersSectionView()
    }
}

private struct ManagerPriorityDropDelegate: DropDelegate {
    let core: HelmCore
    let authority: ManagerAuthority
    let targetManagerId: String
    @Binding var draggedManagerId: String?
    @Binding var suppressWindowBackgroundDragging: Bool

    func performDrop(info: DropInfo) -> Bool {
        guard let draggedManagerId else { return false }
        core.moveManagerPriority(
            authority: authority,
            draggedManagerId: draggedManagerId,
            targetManagerId: targetManagerId
        )
        self.draggedManagerId = nil
        suppressWindowBackgroundDragging = true
        return true
    }

    func dropExited(info: DropInfo) {
        if !info.hasItemsConforming(to: [UTType.text.identifier]) {
            draggedManagerId = nil
            suppressWindowBackgroundDragging = true
        }
    }
}
