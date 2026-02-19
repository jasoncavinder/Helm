import SwiftUI

struct RedesignOverviewSectionView: View {
    @ObservedObject private var core = HelmCore.shared
    @EnvironmentObject private var context: ControlCenterContext

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                HStack {
                    Text(ControlCenterSection.overview.title)
                        .font(.title2.weight(.semibold))
                    Spacer()
                    HealthBadgeView(status: core.aggregateHealth)
                }

                HStack(spacing: 14) {
                    MetricCardView(
                        title: L10n.App.Popover.pendingUpdates.localized,
                        value: core.outdatedPackages.count
                    )
                    MetricCardView(
                        title: L10n.App.Popover.failures.localized,
                        value: core.failedTaskCount
                    )
                    MetricCardView(
                        title: L10n.App.Popover.runningTasks.localized,
                        value: core.runningTaskCount
                    )
                }

                Text(L10n.App.Overview.managerHealth.localized)
                    .font(.headline)

                LazyVGrid(columns: [GridItem(.adaptive(minimum: 220), spacing: 12)], spacing: 12) {
                    ForEach(core.visibleManagers) { manager in
                        ManagerHealthCardView(
                            title: localizedManagerDisplayName(manager.id),
                            authority: manager.authority,
                            status: core.health(forManagerId: manager.id),
                            outdatedCount: core.outdatedCount(forManagerId: manager.id)
                        )
                        .onTapGesture {
                            context.selectedManagerId = manager.id
                            context.selectedPackageId = nil
                            context.selectedTaskId = nil
                        }
                        .helmPointer()
                    }
                }

                Text(L10n.App.Overview.recentTasks.localized)
                    .font(.headline)

                if core.activeTasks.isEmpty {
                    Text(L10n.App.Tasks.noRecentTasks.localized)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                } else {
                    VStack(spacing: 0) {
                        ForEach(Array(core.activeTasks.prefix(10))) { task in
                            TaskRowView(task: task, onCancel: task.isRunning ? { core.cancelTask(task) } : nil)
                                .contentShape(Rectangle())
                                .onTapGesture {
                                    context.selectedTaskId = task.id
                                    context.selectedPackageId = nil
                                    context.selectedManagerId = task.managerId
                                }
                                .helmPointer()
                            Divider()
                        }
                    }
                    .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                }
            }
            .padding(20)
        }
    }
}

struct RedesignUpdatesSectionView: View {
    @ObservedObject private var core = HelmCore.shared
    @EnvironmentObject private var context: ControlCenterContext
    @State private var includeOsUpdates = false
    @State private var showDryRun = false
    @State private var dryRunMessage = ""

    private var previewBreakdown: [(manager: String, count: Int)] {
        core.upgradeAllPreviewBreakdown(includePinned: false, allowOsUpdates: includeOsUpdates)
    }

    private var totalCount: Int {
        previewBreakdown.reduce(0) { $0 + $1.count }
    }

    private var stageRows: [(authority: ManagerAuthority, managerCount: Int, packageCount: Int)] {
        ManagerAuthority.allCases.map { authorityLevel in
            let managersInAuthority = Set(
                previewBreakdown
                    .map(\.manager)
                    .filter { (ManagerInfo.find(byDisplayName: $0)?.authority ?? .standard) == authorityLevel }
            )
            let count = previewBreakdown
                .filter { (ManagerInfo.find(byDisplayName: $0.manager)?.authority ?? .standard) == authorityLevel }
                .reduce(0) { $0 + $1.count }

            return (authority: authorityLevel, managerCount: managersInAuthority.count, packageCount: count)
        }
    }

    private var requiresPrivileges: Bool {
        previewBreakdown.contains { entry in
            entry.manager == localizedManagerDisplayName("homebrew_formula")
                || entry.manager == localizedManagerDisplayName("softwareupdate")
        }
    }

    private var mayRequireReboot: Bool {
        core.outdatedPackages.contains { $0.restartRequired || $0.managerId == "softwareupdate" }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack {
                Text(ControlCenterSection.updates.title)
                    .font(.title2.weight(.semibold))
                Spacer()
                Button(L10n.App.Action.refreshPlan.localized) {
                    core.triggerRefresh()
                }
                .buttonStyle(HelmSecondaryButtonStyle())
                .disabled(core.isRefreshing)
            }

            Text(L10n.App.Updates.executionPlan.localized)
                .font(.headline)

            if !core.safeModeEnabled {
                Toggle(L10n.App.Updates.includeOs.localized, isOn: $includeOsUpdates)
                    .toggleStyle(.switch)
            }

            VStack(spacing: 8) {
                ForEach(stageRows, id: \.authority) { row in
                    HStack {
                        Text(row.authority.key.localized)
                            .font(.body.weight(.medium))
                        Spacer()
                        Text("\(row.managerCount)")
                            .font(.body.monospacedDigit())
                        Text(L10n.App.Updates.managers.localized)
                            .foregroundStyle(.secondary)
                        Text("\(row.packageCount)")
                            .font(.body.monospacedDigit())
                        Text(L10n.App.Updates.packages.localized)
                            .foregroundStyle(.secondary)
                    }
                    .padding(.vertical, 4)
                }
            }

            VStack(alignment: .leading, spacing: 6) {
                Text(L10n.App.Updates.riskFlags.localized)
                    .font(.headline)
                riskRow(flag: L10n.App.Updates.Risk.privileged.localized, active: requiresPrivileges)
                riskRow(flag: L10n.App.Updates.Risk.reboot.localized, active: mayRequireReboot)
            }

            HStack {
                Button(L10n.App.Action.dryRun.localized) {
                    let lines = previewBreakdown.prefix(8).map { "\($0.manager): \($0.count)" }
                    dryRunMessage = L10n.App.DryRun.message.localized(with: [
                        "count": totalCount,
                        "summary": lines.joined(separator: "\n")
                    ])
                    showDryRun = true
                }
                .buttonStyle(HelmSecondaryButtonStyle())

                Button(L10n.App.Action.runPlan.localized) {
                    core.upgradeAll(includePinned: false, allowOsUpdates: includeOsUpdates)
                }
                .buttonStyle(HelmPrimaryButtonStyle())
                .disabled(totalCount == 0)

                Spacer()
            }

            Spacer()
        }
        .padding(20)
        .alert(L10n.App.DryRun.title.localized, isPresented: $showDryRun) {
            Button(L10n.Common.ok.localized, role: .cancel) {}
        } message: {
            Text(dryRunMessage)
        }
    }

    private func riskRow(flag: String, active: Bool) -> some View {
        HStack(spacing: 8) {
            Image(systemName: active ? "checkmark.circle.fill" : "circle")
                .foregroundStyle(active ? Color.orange : Color.secondary)
            Text(flag)
                .font(.subheadline)
                .foregroundStyle(active ? Color.primary : Color.secondary)
        }
    }
}

struct RedesignUpgradeSheetView: View {
    @ObservedObject private var core = HelmCore.shared
    @EnvironmentObject private var context: ControlCenterContext
    @Environment(\.dismiss) private var dismiss
    @State private var includeOsUpdates = false

    private var noOsCount: Int {
        core.upgradeAllPreviewCount(includePinned: false, allowOsUpdates: false)
    }

    private var withOsCount: Int {
        core.upgradeAllPreviewCount(includePinned: false, allowOsUpdates: true)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(L10n.App.Updates.executionPlan.localized)
                .font(.title3.weight(.semibold))

            if !core.safeModeEnabled {
                Toggle(L10n.App.Updates.includeOs.localized, isOn: $includeOsUpdates)
                    .toggleStyle(.switch)
            }

            HStack {
                Text(L10n.App.Updates.Authority.standard.localized)
                Spacer()
                Text("\(includeOsUpdates ? withOsCount : noOsCount)")
                    .font(.callout.monospacedDigit())
            }

            Divider()

            HStack {
                Button(L10n.Common.cancel.localized) {
                    context.showUpgradeSheet = false
                    dismiss()
                }
                .buttonStyle(HelmSecondaryButtonStyle())
                Spacer()
                Button(L10n.App.Action.dryRun.localized) {}
                    .buttonStyle(HelmSecondaryButtonStyle())
                    .disabled(true)
                Button(L10n.App.Action.runPlan.localized) {
                    core.upgradeAll(includePinned: false, allowOsUpdates: includeOsUpdates)
                    context.showUpgradeSheet = false
                    dismiss()
                }
                .buttonStyle(HelmPrimaryButtonStyle())
                .disabled((includeOsUpdates ? withOsCount : noOsCount) == 0)
            }
        }
        .padding(20)
        .frame(minWidth: 460)
    }
}

struct MetricCardView: View {
    let title: String
    let value: Int

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title)
                .font(.caption)
                .foregroundStyle(.secondary)
            Text("\(value)")
                .font(.title3.monospacedDigit().weight(.semibold))
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .accessibilityElement(children: .combine)
        .accessibilityLabel(title)
        .accessibilityValue("\(value)")
    }
}

struct ManagerHealthCardView: View {
    let title: String
    let authority: ManagerAuthority
    let status: OperationalHealth
    let outdatedCount: Int

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text(title)
                    .font(.headline)
                Spacer()
                HealthBadgeView(status: status)
            }

            HStack(spacing: 6) {
                Text(authority.key.localized)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Text("|")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Text("\(outdatedCount)")
                    .font(.caption.monospacedDigit())
                Text(L10n.App.Packages.Filter.upgradable.localized)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(title), \(authority.key.localized)")
        .accessibilityValue("\(status.key.localized), \(outdatedCount) \(L10n.App.Packages.Filter.upgradable.localized)")
    }
}
