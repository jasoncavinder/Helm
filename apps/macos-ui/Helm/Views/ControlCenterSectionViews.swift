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
                            context.selectedUpgradePlanStepId = nil
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
                                    context.selectedUpgradePlanStepId = nil
                                }
                                .helmPointer()
                            Divider()
                        }
                    }
                    .helmCardSurface(cornerRadius: 12)
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
    @State private var managerScopeId = HelmCore.allManagersScopeId
    @State private var packageScopeQuery = ""

    private var totalCount: Int {
        scopedPlanSteps.count
    }

    private var managerScopeOptions: [String] {
        let managers = Set(core.upgradePlanSteps.map(\.managerId))
        return [HelmCore.allManagersScopeId] + managers.sorted()
    }

    private var scopedPlanSteps: [CoreUpgradePlanStep] {
        HelmCore.scopedUpgradePlanSteps(
            from: core.upgradePlanSteps,
            managerScopeId: managerScopeId,
            packageFilter: packageScopeQuery
        )
    }

    private var visiblePlanSteps: [CoreUpgradePlanStep] {
        Array(scopedPlanSteps.prefix(80))
    }

    private var scopedInFlightStepCount: Int {
        scopedPlanSteps.filter { step in
            let hasProjectedTask = core.upgradePlanTaskProjectionByStepId[step.id] != nil
            return UpgradePreviewPlanner.isInFlightStatus(
                status: projectedStatus(step),
                hasProjectedTask: hasProjectedTask
            )
        }.count
    }

    private var stageRows: [(authority: ManagerAuthority, managerCount: Int, packageCount: Int)] {
        let stepsByAuthority = Dictionary(grouping: scopedPlanSteps) { step in
            authority(for: step.managerId)
        }
        return ManagerAuthority.allCases.map { authorityLevel in
            let scopedSteps = stepsByAuthority[authorityLevel] ?? []
            let managersInAuthority = Set(scopedSteps.map(\.managerId))
            return (
                authority: authorityLevel,
                managerCount: managersInAuthority.count,
                packageCount: scopedSteps.count
            )
        }
    }

    private var requiresPrivileges: Bool {
        scopedPlanSteps.contains { step in
            step.managerId == "homebrew_formula" || step.managerId == "softwareupdate"
        }
    }

    private func planStepTitle(_ step: CoreUpgradePlanStep) -> String {
        if step.managerId == "softwareupdate", step.packageName == "__confirm_os_updates__" {
            return core.localizedUpgradePlanReason(for: step)
        }
        return step.packageName
    }

    private func projectedStatus(_ step: CoreUpgradePlanStep) -> String {
        core.projectedUpgradePlanStatus(for: step)
    }

    private func packageSummary(_ packageNames: [String], managerId: String) -> String {
        packageNames
            .prefix(4)
            .map { package in
                if managerId == "softwareupdate", package == "__confirm_os_updates__" {
                    return L10n.Service.Task.Label.upgradeSoftwareUpdateAll.localized
                }
                return package
            }
            .joined(separator: ", ")
    }

    private var mayRequireReboot: Bool {
        scopedPlanSteps.contains { step in
            if step.managerId == "softwareupdate" {
                return true
            }
            return core.outdatedPackages.contains { pkg in
                pkg.managerId == step.managerId && pkg.name == step.packageName && pkg.restartRequired
            }
        }
    }

    private var scopedFailedStepIds: [String] {
        scopedPlanSteps
            .filter { projectedStatus($0).lowercased() == "failed" }
            .map(\.id)
    }

    private var scopedFailureGroups: [UpgradePlanFailureGroup] {
        let scopedSet = Set(scopedPlanSteps.map(\.id))
        return core.upgradePlanFailureGroups.compactMap { group in
            let scopedIds = group.stepIds.filter { scopedSet.contains($0) }
            guard !scopedIds.isEmpty else { return nil }
            let scopedPackages = core.upgradePlanSteps
                .filter { scopedIds.contains($0.id) }
                .map(\.packageName)
            return UpgradePlanFailureGroup(
                id: group.id,
                managerId: group.managerId,
                stepIds: scopedIds,
                packageNames: scopedPackages
            )
        }
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                HStack {
                    Text(ControlCenterSection.updates.title)
                        .font(.title2.weight(.semibold))
                    Spacer()
                    Button(L10n.App.Action.refreshPlan.localized) {
                        core.triggerRefresh()
                        core.refreshUpgradePlan(includePinned: false, allowOsUpdates: includeOsUpdates)
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

                HStack(spacing: 10) {
                    Picker(L10n.App.Inspector.manager.localized, selection: $managerScopeId) {
                        ForEach(managerScopeOptions, id: \.self) { managerId in
                            if managerId == HelmCore.allManagersScopeId {
                                Text(L10n.App.Packages.Filter.allManagers.localized)
                                    .tag(managerId)
                            } else {
                                Text(localizedManagerDisplayName(managerId))
                                    .tag(managerId)
                            }
                        }
                    }
                    .frame(maxWidth: 240)

                    TextField(L10n.App.ControlCenter.searchPlaceholder.localized, text: $packageScopeQuery)
                        .textFieldStyle(.roundedBorder)
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

                if core.scopedUpgradePlanRunInProgress {
                    HStack(spacing: 8) {
                        ProgressView()
                            .controlSize(.small)
                        Text(L10n.App.Managers.Operation.upgrading.localized)
                            .font(.callout.weight(.medium))
                        Spacer()
                        Text("\(scopedInFlightStepCount)")
                            .font(.caption.monospacedDigit())
                            .foregroundStyle(.secondary)
                    }
                    .padding(.horizontal, 10)
                    .padding(.vertical, 8)
                    .helmCardSurface(cornerRadius: 10, highlighted: true)
                }

                if visiblePlanSteps.isEmpty {
                    Text(L10n.App.Tasks.noRecentTasks.localized)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                } else {
                    VStack(spacing: 0) {
                        ForEach(Array(visiblePlanSteps.enumerated()), id: \.element.id) { index, step in
                            Button {
                                context.selectedUpgradePlanStepId = step.id
                                context.selectedTaskId = nil
                                context.selectedPackageId = nil
                                context.selectedManagerId = nil
                            } label: {
                                HStack(spacing: 8) {
                                    Text("\(index + 1).")
                                        .font(.caption.monospacedDigit())
                                        .foregroundStyle(.secondary)
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(planStepTitle(step))
                                            .font(.subheadline.weight(.medium))
                                            .lineLimit(1)
                                        Text(localizedManagerDisplayName(step.managerId))
                                            .font(.caption)
                                            .foregroundStyle(.secondary)
                                            .lineLimit(1)
                                    }
                                    Spacer()
                                    Text(core.localizedUpgradePlanStatus(projectedStatus(step)))
                                        .font(.caption)
                                        .foregroundStyle(
                                            projectedStatus(step).lowercased() == "failed"
                                                ? Color.red
                                                : Color.secondary
                                        )
                                }
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .padding(.vertical, 8)
                                .padding(.horizontal, 10)
                                .background(
                                    context.selectedUpgradePlanStepId == step.id
                                        ? HelmTheme.selectionFill
                                        : Color.clear
                                )
                            }
                            .buttonStyle(.plain)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .contentShape(Rectangle())
                            .helmPointer()
                            Divider()
                        }
                    }
                    .helmCardSurface(cornerRadius: 12)
                }

                VStack(alignment: .leading, spacing: 6) {
                    Text(L10n.App.Updates.riskFlags.localized)
                        .font(.headline)
                    riskRow(flag: L10n.App.Updates.Risk.privileged.localized, active: requiresPrivileges)
                    riskRow(flag: L10n.App.Updates.Risk.reboot.localized, active: mayRequireReboot)
                }

                if !scopedFailureGroups.isEmpty {
                    VStack(alignment: .leading, spacing: 10) {
                        Text(L10n.App.Popover.failures.localized)
                            .font(.headline)

                        ForEach(scopedFailureGroups) { group in
                            VStack(alignment: .leading, spacing: 6) {
                                Text(core.localizedUpgradePlanFailureCause(for: group))
                                    .font(.caption)
                                    .foregroundStyle(.secondary)

                                Text(packageSummary(group.packageNames, managerId: group.managerId))
                                    .font(.caption.monospaced())
                                    .lineLimit(2)

                                HStack {
                                    Button(L10n.App.Packages.Action.update.localized) {
                                        core.retryUpgradePlanSteps(stepIds: group.stepIds)
                                    }
                                    .buttonStyle(HelmSecondaryButtonStyle())
                                    .font(.caption)
                                    Spacer()
                                    Text(localizedManagerDisplayName(group.managerId))
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                }
                            }
                            .padding(10)
                            .helmCardSurface(cornerRadius: 10, highlighted: true)
                        }

                        Button(L10n.App.Packages.Action.update.localized) {
                            core.retryUpgradePlanSteps(stepIds: scopedFailedStepIds)
                        }
                        .buttonStyle(HelmPrimaryButtonStyle())
                        .font(.caption)
                        .disabled(scopedFailedStepIds.isEmpty)
                    }
                }

                HStack {
                    Button(L10n.App.Tasks.Action.cancel.localized) {
                        core.cancelRemainingUpgradePlanSteps(
                            managerScopeId: managerScopeId,
                            packageFilter: packageScopeQuery
                        )
                    }
                    .buttonStyle(HelmSecondaryButtonStyle())

                    Button(L10n.App.Action.runPlan.localized) {
                        core.runUpgradePlanScoped(
                            managerScopeId: managerScopeId,
                            packageFilter: packageScopeQuery
                        )
                    }
                    .buttonStyle(HelmPrimaryButtonStyle())
                    .disabled(totalCount == 0 || core.scopedUpgradePlanRunInProgress)

                    Spacer()
                }
            }
            .padding(20)
        }
        .onAppear {
            core.refreshUpgradePlan(includePinned: false, allowOsUpdates: includeOsUpdates)
        }
        .onChange(of: includeOsUpdates) { value in
            core.refreshUpgradePlan(includePinned: false, allowOsUpdates: value)
        }
        .onChange(of: core.safeModeEnabled) { _ in
            core.refreshUpgradePlan(includePinned: false, allowOsUpdates: includeOsUpdates)
        }
        .onChange(of: core.upgradePlanSteps) { steps in
            let managerSet = Set(steps.map(\.managerId))
            if managerScopeId != HelmCore.allManagersScopeId && !managerSet.contains(managerScopeId) {
                managerScopeId = HelmCore.allManagersScopeId
            }
        }
    }

    private func riskRow(flag: String, active: Bool) -> some View {
        HStack(spacing: 8) {
            Image(systemName: active ? "checkmark.circle.fill" : "circle")
                .foregroundStyle(active ? HelmTheme.stateAttention : HelmTheme.textSecondary)
            Text(flag)
                .font(.subheadline)
                .foregroundStyle(active ? HelmTheme.textPrimary : HelmTheme.textSecondary)
        }
    }
}

struct RedesignUpgradeSheetView: View {
    @ObservedObject private var core = HelmCore.shared
    @EnvironmentObject private var context: ControlCenterContext
    @Environment(\.presentationMode) private var presentationMode
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
                    presentationMode.wrappedValue.dismiss()
                }
                .buttonStyle(HelmSecondaryButtonStyle())
                Spacer()
                Button(L10n.App.Action.dryRun.localized) {}
                    .buttonStyle(HelmSecondaryButtonStyle())
                    .disabled(true)
                Button(L10n.App.Action.runPlan.localized) {
                    core.upgradeAll(includePinned: false, allowOsUpdates: includeOsUpdates)
                    context.showUpgradeSheet = false
                    presentationMode.wrappedValue.dismiss()
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
        .helmCardSurface(cornerRadius: 12)
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
        .helmCardSurface(cornerRadius: 12)
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(title), \(authority.key.localized)")
        .accessibilityValue("\(status.key.localized), \(outdatedCount) \(L10n.App.Packages.Filter.upgradable.localized)")
    }
}
