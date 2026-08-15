import SwiftUI

struct RedesignOverviewSectionView: View {
    private let core = HelmCore.shared
    @ObservedObject private var overviewState = HelmCore.shared.overviewState
    @EnvironmentObject private var context: ControlCenterContext
    @State private var expandedTaskId: String?

    private var projection: WayfinderProjectionContent {
        overviewState.wayfinderProjection.content
    }

    var body: some View {
        ScrollViewReader { scrollProxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 18) {
                    WayfinderDashboardHero(
                        projection: projection,
                        isRefreshing: core.isRefreshing,
                        onPrimaryAction: {
                            context.navigate(to: projection.primaryAction)
                        },
                        onRefresh: {
                            core.triggerRefresh()
                        }
                    )

                    DashboardServiceHealthCard()
                        .id(WayfinderFocusTarget.serviceHealth.rawValue)

                    Text(L10n.App.Overview.managerHealth.localized)
                        .font(.headline)

                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 220), spacing: 12)], spacing: 12) {
                        ForEach(overviewState.visibleManagers) { manager in
                            ManagerHealthCardView(
                                title: localizedManagerDisplayName(manager.id),
                                authority: manager.authority,
                                status: overviewState.managerHealthById[manager.id] ?? .healthy,
                                outdatedCount: overviewState.outdatedCountByManager[manager.id, default: 0],
                                isSelected: context.selectedManagerId == manager.id
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

                    if overviewState.recentTasksTop10.isEmpty {
                        Text(L10n.App.Tasks.noRecentTasks.localized)
                            .font(.subheadline)
                            .foregroundColor(.secondary)
                    } else {
                        VStack(spacing: 0) {
                            ForEach(overviewState.recentTasksTop10) { task in
                                TaskRowView(
                                    task: task,
                                    onCancel: task.isRunning ? { core.cancelTask(task) } : nil,
                                    onDismiss: task.status.lowercased() == "failed" ? { core.dismissTask(task) } : nil,
                                    canExpandDetails: task.supportsInlineDetails,
                                    isExpanded: expandedTaskId == task.id,
                                    isSelected: context.selectedTaskId == task.id,
                                    onToggleDetails: {
                                        if expandedTaskId == task.id {
                                            expandedTaskId = nil
                                        } else {
                                            expandedTaskId = task.id
                                        }
                                    },
                                    onSelect: {
                                        context.selectedTaskId = task.id
                                        context.selectedPackageId = nil
                                        context.selectedManagerId = task.managerId
                                        context.selectedUpgradePlanStepId = nil
                                        if !task.supportsInlineDetails {
                                            expandedTaskId = nil
                                        }
                                    }
                                )
                                Divider()
                            }
                        }
                        .helmCardSurface(cornerRadius: 12)
                    }
                }
                .padding(20)
            }
            .onAppear {
                fulfillDashboardFocusRequest(using: scrollProxy)
            }
            .onChange(of: context.dashboardFocusRequestToken) { _ in
                fulfillDashboardFocusRequest(using: scrollProxy)
            }
            .onChange(of: overviewState.recentTasksTop10.map { "\($0.id):\($0.status)" }) { _ in
                collapseExpandedTaskIfNeeded()
            }
            .onChange(of: context.selectedTaskId) { selectedTaskId in
                if expandedTaskId != selectedTaskId {
                    expandedTaskId = nil
                }
            }
        }
    }

    private func fulfillDashboardFocusRequest(using scrollProxy: ScrollViewProxy) {
        guard context.takeDashboardFocusRequest() == .serviceHealth else { return }
        DispatchQueue.main.async {
            withAnimation(.easeInOut(duration: 0.25)) {
                scrollProxy.scrollTo(WayfinderFocusTarget.serviceHealth.rawValue, anchor: .top)
            }
        }
    }

    private func collapseExpandedTaskIfNeeded() {
        guard let expandedTaskId else { return }
        let stillVisible = overviewState.recentTasksTop10.contains {
            $0.id == expandedTaskId && $0.supportsInlineDetails
        }
        if !stillVisible {
            self.expandedTaskId = nil
        }
    }
}

private struct DashboardServiceHealthCard: View {
    @ObservedObject private var core = HelmCore.shared
    @ObservedObject private var overviewState = HelmCore.shared.overviewState
    @State private var showCopiedConfirmation = false

    private var connectionStatus: String {
        core.isConnected
            ? L10n.App.Settings.ServiceHealth.Status.connected.localized
            : L10n.App.Settings.ServiceHealth.Status.disconnected.localized
    }

    private var refreshStatus: String {
        core.isRefreshing
            ? L10n.App.Settings.ServiceHealth.Status.refreshing.localized
            : L10n.App.Settings.ServiceHealth.Status.idle.localized
    }

    private var managerCounts: DashboardManagerCounts {
        DashboardManagerCounts(
            statuses: core.managerStatuses.values.map {
                (
                    detected: $0.detected,
                    enabled: $0.enabled,
                    isImplemented: $0.isImplemented
                )
            }
        )
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Label(L10n.App.Settings.ServiceHealth.section.localized, systemImage: "stethoscope")
                    .font(.headline)
                Spacer()
                HealthBadgeView(status: overviewState.aggregateHealth)
            }

            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 190), spacing: 10)],
                alignment: .leading,
                spacing: 10
            ) {
                DashboardServiceHealthMetric(
                    title: L10n.App.Settings.ServiceHealth.connection.localized,
                    value: connectionStatus
                )
                DashboardServiceHealthMetric(
                    title: L10n.App.Settings.ServiceHealth.refreshState.localized,
                    value: refreshStatus
                )
                DashboardServiceHealthMetric(
                    title: L10n.App.Settings.ServiceHealth.failedTasks.localized,
                    value: "\(overviewState.failedTaskCount)"
                )
                DashboardServiceHealthMetric(
                    title: L10n.App.Settings.ServiceHealth.managerCoverage.localized,
                    value: L10n.App.Settings.ServiceHealth.managerCoverageValue.localized(with: [
                        "detected": managerCounts.detected,
                        "disabled": managerCounts.disabled,
                    ])
                )
                DashboardServiceHealthMetric(
                    title: L10n.App.Settings.ServiceHealth.otherManagers.localized,
                    value: L10n.App.Settings.ServiceHealth.otherManagersValue.localized(with: [
                        "available": managerCounts.available,
                    ])
                )
            }

            if let lastError = core.lastError, !lastError.isEmpty {
                VStack(alignment: .leading, spacing: 3) {
                    Text(L10n.App.Settings.ServiceHealth.lastError.localized)
                        .font(.caption.weight(.semibold))
                        .foregroundColor(HelmTheme.stateAttention)
                    Text(lastError)
                        .font(.caption)
                        .foregroundColor(HelmTheme.textSecondary)
                        .lineLimit(3)
                        .textSelection(.enabled)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(10)
                .background(HelmTheme.stateAttention.opacity(0.08), in: RoundedRectangle(cornerRadius: 8))
            }

            HStack(spacing: 10) {
                Button(L10n.App.Settings.ServiceHealth.copySnapshot.localized) {
                    HelmSupport.copyServiceHealthDiagnosticsToClipboard()
                    showCopiedBriefly()
                }
                .buttonStyle(HelmSecondaryButtonStyle())

                if showCopiedConfirmation {
                    Label(
                        L10n.App.Settings.SupportFeedback.copiedConfirmation.localized,
                        systemImage: "checkmark.circle.fill"
                    )
                    .font(.caption)
                    .foregroundColor(HelmTheme.stateHealthy)
                    .transition(.opacity.combined(with: .scale))
                }

                Spacer()
            }
        }
        .padding(16)
        .helmCardSurface(cornerRadius: 12)
    }

    private func showCopiedBriefly() {
        withAnimation(.easeInOut(duration: 0.2)) {
            showCopiedConfirmation = true
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 2.0) {
            withAnimation(.easeInOut(duration: 0.3)) {
                showCopiedConfirmation = false
            }
        }
    }
}

private struct DashboardServiceHealthMetric: View {
    let title: String
    let value: String

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(title)
                .font(.caption)
                .foregroundColor(HelmTheme.textSecondary)
            Text(value)
                .font(.subheadline.monospacedDigit().weight(.medium))
                .foregroundColor(HelmTheme.textPrimary)
                .lineLimit(1)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(10)
        .background(HelmTheme.surfaceElevated, in: RoundedRectangle(cornerRadius: 8))
        .accessibilityElement(children: .combine)
        .accessibilityLabel(title)
        .accessibilityValue(value)
    }
}

struct RedesignUpdatesSectionView: View {
    private struct PlanStageRow: Identifiable {
        let id: String
        let labelKey: String
        let managerCount: Int
        let packageCount: Int
    }

    @ObservedObject private var core = HelmCore.shared
    @ObservedObject private var appUpdate = AppUpdateCoordinator.shared
    @EnvironmentObject private var context: ControlCenterContext
    @State private var includeOsUpdates = false
    @State private var managerScopeId = HelmCore.allManagersScopeId
    @State private var packageScopeQuery = ""
    @State private var failedExternalSparkleStep: CoreUpgradePlanStep?
    @State private var selectedPlanStepIds = Set<String>()
    @State private var knownPlanStepIds = Set<String>()

    private var runnableCount: Int {
        selectedScopedPlanSteps.filter(core.upgradePlanStepRunsAutomatically).count
    }

    private var interactiveSparkleCount: Int {
        selectedScopedPlanSteps.filter(HelmCore.isExternalSparklePlanStep).count
    }

    private var managerScopeOptions: [String] {
        let managers = Set(core.upgradePlanSteps.map(\.managerId))
        let enabledManagers = managers.filter { core.isManagerEnabled($0) }
        return [HelmCore.allManagersScopeId] + enabledManagers.sorted()
    }

    private var scopedPlanSteps: [CoreUpgradePlanStep] {
        HelmCore.scopedUpgradePlanSteps(
            from: core.upgradePlanSteps,
            managerScopeId: managerScopeId,
            packageFilter: packageScopeQuery
        )
    }

    private var visiblePlanSteps: [CoreUpgradePlanStep] {
        scopedPlanSteps
    }

    private var selectedScopedPlanSteps: [CoreUpgradePlanStep] {
        scopedPlanSteps.filter { selectedPlanStepIds.contains($0.id) }
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

    private var stageRows: [PlanStageRow] {
        let stages = [
            (id: "authoritative", labelKey: L10n.App.Updates.Authority.authoritative),
            (id: "standard", labelKey: L10n.App.Updates.Authority.standard),
            (id: "guarded", labelKey: L10n.App.Updates.Authority.guarded),
            (id: "interactive", labelKey: L10n.App.Updates.Authority.interactive),
        ]
        let stepsByAuthority = Dictionary(grouping: scopedPlanSteps) { step in
            let normalized = step.authority.lowercased()
            return stages.contains(where: { $0.id == normalized }) ? normalized : "standard"
        }
        return stages.map { stage in
            let scopedSteps = stepsByAuthority[stage.id] ?? []
            let managersInAuthority = Set(scopedSteps.map(\.managerId))
            return PlanStageRow(
                id: stage.id,
                labelKey: stage.labelKey,
                managerCount: managersInAuthority.count,
                packageCount: scopedSteps.count
            )
        }
    }

    private var riskSummary: UpgradePreviewPlanner.RiskSummary {
        UpgradePreviewPlanner.riskSummary(
            for: selectedScopedPlanSteps.map {
                .init(managerId: $0.managerId, packageName: $0.packageName)
            },
            restartRequiredCandidates: core.outdatedPackages
                .filter(\.restartRequired)
                .map { .init(managerId: $0.managerId, packageName: $0.name) }
        )
    }

    private func planStepTitle(_ step: CoreUpgradePlanStep) -> String {
        if step.managerId == "softwareupdate", step.packageName == "__confirm_os_updates__" {
            return core.localizedUpgradePlanReason(for: step)
        }
        return step.packageName
    }

    private func selectionBinding(for step: CoreUpgradePlanStep) -> Binding<Bool> {
        Binding(
            get: { selectedPlanStepIds.contains(step.id) },
            set: { included in
                if included {
                    selectedPlanStepIds.insert(step.id)
                } else {
                    selectedPlanStepIds.remove(step.id)
                }
            }
        )
    }

    private func selectInspectorStep(_ step: CoreUpgradePlanStep) {
        context.selectedUpgradePlanStepId = step.id
        context.selectedTaskId = nil
        context.selectedPackageId = nil
        context.selectedManagerId = nil
    }

    private func reconcilePlanSelection(_ steps: [CoreUpgradePlanStep]) {
        let selection = UpgradePreviewPlanner.reconcileSelection(
            selectedStepIds: selectedPlanStepIds,
            knownStepIds: knownPlanStepIds,
            availableStepIds: Set(steps.map(\.id))
        )
        selectedPlanStepIds = selection.selectedStepIds
        knownPlanStepIds = selection.knownStepIds
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
                if managerId == "mas", package == "__all__" {
                    return "service.task.label.upgrade.mas_all".localized
                }
                return package
            }
            .joined(separator: ", ")
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
            LazyVStack(alignment: .leading, spacing: 16) {
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

                if let completion = core.upgradePlanCompletion,
                   completion.completedNormally,
                   completion.remainingInteractiveCount > 0 {
                    VStack(alignment: .leading, spacing: 4) {
                        Label(
                            L10n.App.Updates.Completion.title.localized,
                            systemImage: "checkmark.circle.fill"
                        )
                        .font(.callout.weight(.semibold))
                        Text(
                            L10n.App.Updates.Completion.message.localized(with: [
                                "count": completion.remainingInteractiveCount
                            ])
                        )
                        .font(.callout)
                        .foregroundColor(.secondary)
                    }
                    .padding(10)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .helmCardSurface(cornerRadius: 10, highlighted: true)
                } else if interactiveSparkleCount > 0 {
                    Label(
                        L10n.App.Updates.interactiveSparkleNotice.localized(with: [
                            "count": interactiveSparkleCount
                        ]),
                        systemImage: "hand.raised.fill"
                    )
                    .font(.callout)
                    .foregroundColor(.secondary)
                    .padding(10)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .helmCardSurface(cornerRadius: 10, highlighted: true)
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
                    ForEach(stageRows) { row in
                        HStack {
                            Text(row.labelKey.localized)
                                .font(.body.weight(.medium))
                            Spacer()
                            Text("\(row.managerCount)")
                                .font(.body.monospacedDigit())
                            Text(L10n.App.Updates.managers.localized)
                                .foregroundColor(.secondary)
                            Text("\(row.packageCount)")
                                .font(.body.monospacedDigit())
                            Text(L10n.App.Updates.packages.localized)
                                .foregroundColor(.secondary)
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
                            .foregroundColor(.secondary)
                    }
                    .padding(.horizontal, 10)
                    .padding(.vertical, 8)
                    .helmCardSurface(cornerRadius: 10, highlighted: true)
                }

                if visiblePlanSteps.isEmpty {
                    Text(L10n.App.Tasks.noRecentTasks.localized)
                        .font(.subheadline)
                        .foregroundColor(.secondary)
                } else {
                    LazyVStack(spacing: 0) {
                        ForEach(Array(visiblePlanSteps.enumerated()), id: \.element.id) { index, step in
                            HStack(spacing: 8) {
                                Toggle("", isOn: selectionBinding(for: step))
                                    .labelsHidden()
                                    .toggleStyle(.checkbox)
                                    .disabled(core.scopedUpgradePlanRunInProgress)

                                Text("\(index + 1).")
                                    .font(.caption.monospacedDigit())
                                    .foregroundColor(.secondary)
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(planStepTitle(step))
                                        .font(.subheadline.weight(.medium))
                                        .lineLimit(1)
                                    Text(localizedManagerDisplayName(step.managerId))
                                        .font(.caption)
                                        .foregroundColor(.secondary)
                                        .lineLimit(1)
                                }
                                Spacer()
                                Text(core.localizedUpgradePlanStatus(projectedStatus(step)))
                                    .font(.caption)
                                    .foregroundColor(
                                        projectedStatus(step).lowercased() == "failed"
                                            ? Color.red
                                            : (
                                                core.upgradePlanStepRunsAutomatically(step)
                                                    ? Color.secondary
                                                    : HelmTheme.stateAttention
                                            )
                                    )

                                if HelmCore.isExternalSparklePlanStep(step) {
                                    Button(L10n.App.Updates.openAppToUpdate.localized) {
                                        if !core.openExternalSparkleApplication(for: step) {
                                            failedExternalSparkleStep = step
                                        }
                                    }
                                    .buttonStyle(HelmSecondaryButtonStyle())
                                    .font(.caption)
                                    .helmPointer()
                                }
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(.horizontal, 12)
                            .padding(.vertical, 8)
                            .contentShape(Rectangle())
                            .onTapGesture {
                                selectInspectorStep(step)
                            }
                            .helmPointer()
                            .background(
                                context.selectedUpgradePlanStepId == step.id
                                    ? HelmTheme.selectionFill
                                    : Color.clear
                            )
                            Divider()
                        }
                    }
                    .helmCardSurface(cornerRadius: 12)
                }

                VStack(alignment: .leading, spacing: 6) {
                    Text(L10n.App.Updates.riskFlags.localized)
                        .font(.headline)
                    riskRow(
                        flag: L10n.App.Updates.Risk.privileged.localized,
                        active: riskSummary.requiresElevatedPrivileges
                    )
                    riskRow(
                        flag: L10n.App.Updates.Risk.reboot.localized,
                        active: riskSummary.mayRequireReboot
                    )
                }

                if !scopedFailureGroups.isEmpty {
                    VStack(alignment: .leading, spacing: 10) {
                        Text(L10n.App.Popover.failures.localized)
                            .font(.headline)

                        ForEach(scopedFailureGroups) { group in
                            VStack(alignment: .leading, spacing: 6) {
                                Text(core.localizedUpgradePlanFailureCause(for: group))
                                    .font(.caption)
                                    .foregroundColor(.secondary)

                                Text(packageSummary(group.packageNames, managerId: group.managerId))
                                    .font(.caption.monospacedDigit())
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
                                        .foregroundColor(.secondary)
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
                            packageFilter: packageScopeQuery,
                            selectedStepIds: selectedPlanStepIds
                        )
                    }
                    .buttonStyle(HelmPrimaryButtonStyle())
                    .disabled(runnableCount == 0 || core.scopedUpgradePlanRunInProgress)

                    Spacer()
                }
            }
            .padding(20)
        }
        .onAppear {
            reconcilePlanSelection(core.upgradePlanSteps)
            core.refreshUpgradePlan(includePinned: false, allowOsUpdates: includeOsUpdates)
        }
        .onChange(of: includeOsUpdates) { value in
            core.refreshUpgradePlan(includePinned: false, allowOsUpdates: value)
        }
        .onChange(of: core.safeModeEnabled) { _ in
            core.refreshUpgradePlan(includePinned: false, allowOsUpdates: includeOsUpdates)
        }
        .onChange(of: core.upgradePlanSteps) { steps in
            reconcilePlanSelection(steps)
            let managerSet = Set(steps.map(\.managerId))
            if managerScopeId != HelmCore.allManagersScopeId && !managerSet.contains(managerScopeId) {
                managerScopeId = HelmCore.allManagersScopeId
            }
        }
        .onChange(of: appUpdate.includeHelmInUpgradeAll) { _ in
            core.refreshUpgradePlan(includePinned: false, allowOsUpdates: includeOsUpdates)
        }
        .alert(item: $failedExternalSparkleStep) { _ in
            Alert(
                title: Text(L10n.Common.error.localized),
                message: Text(L10n.App.Updates.sparkleOpenFailed.localized),
                dismissButton: .default(Text(L10n.Common.ok.localized))
            )
        }
    }

    private func riskRow(flag: String, active: Bool) -> some View {
        HStack(spacing: 8) {
            Image(systemName: active ? "checkmark.circle.fill" : "circle")
                .foregroundColor(active ? HelmTheme.stateAttention : HelmTheme.textSecondary)
            Text(flag)
                .font(.subheadline)
                .foregroundColor(active ? HelmTheme.textPrimary : HelmTheme.textSecondary)
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
                    context.dismissUpgradeSheet()
                    presentationMode.wrappedValue.dismiss()
                }
                .buttonStyle(HelmSecondaryButtonStyle())
                Spacer()
                Button(L10n.App.Action.runPlan.localized) {
                    core.upgradeAll(includePinned: false, allowOsUpdates: includeOsUpdates)
                    context.dismissUpgradeSheet()
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

struct ManagerHealthCardView: View {
    let title: String
    let authority: ManagerAuthority
    let status: OperationalHealth
    let outdatedCount: Int
    let isSelected: Bool

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
                    .foregroundColor(.secondary)
                Text("|")
                    .font(.caption)
                    .foregroundColor(.secondary)
                Text("\(outdatedCount)")
                    .font(.caption.monospacedDigit())
                Text(L10n.App.Packages.Filter.upgradable.localized)
                    .font(.caption)
                    .foregroundColor(.secondary)
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .helmCardSurface(cornerRadius: 12)
        .overlay(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .fill(isSelected ? HelmTheme.selectionFill : Color.clear)
                .overlay(
                    RoundedRectangle(cornerRadius: 12, style: .continuous)
                        .strokeBorder(isSelected ? HelmTheme.selectionStroke : Color.clear, lineWidth: 0.9)
                )
        )
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(title), \(authority.key.localized)")
        .accessibilityValue("\(status.key.localized), \(outdatedCount) \(L10n.App.Packages.Filter.upgradable.localized)")
    }
}
