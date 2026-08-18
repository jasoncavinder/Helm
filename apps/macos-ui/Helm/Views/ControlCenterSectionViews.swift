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
                        .foregroundColor(HelmTheme.stateError)
                    Text(lastError)
                        .font(.caption)
                        .foregroundColor(HelmTheme.textSecondary)
                        .lineLimit(3)
                        .textSelection(.enabled)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(10)
                .background(HelmTheme.stateError.opacity(0.08), in: RoundedRectangle(cornerRadius: 8))
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

extension WholeWorkflowResearchPlanStep {
    var coreUpgradePlanStep: CoreUpgradePlanStep {
        CoreUpgradePlanStep(
            stepId: id,
            orderIndex: orderIndex,
            managerId: managerID,
            authority: authority,
            action: action,
            packageName: packageName,
            reasonLabelKey: reasonLabelKey,
            reasonLabelArgs: reasonLabelArgs,
            status: status
        )
    }
}

extension CoreUpgradePlanStep {
    func reviewedUpgradePlanStep(status projectedStatus: String? = nil) -> ReviewedUpgradePlanStep {
        ReviewedUpgradePlanStep(
            id: id,
            orderIndex: orderIndex,
            managerID: managerId,
            authority: authority,
            action: action,
            packageName: packageName,
            reasonLabelKey: reasonLabelKey,
            reasonLabelArgs: reasonLabelArgs,
            status: projectedStatus ?? status
        )
    }
}

struct RedesignUpdatesSectionView: View {
    @ObservedObject private var core = HelmCore.shared
    @ObservedObject private var appUpdate = AppUpdateCoordinator.shared
    @ObservedObject private var localization = LocalizationManager.shared
    @EnvironmentObject private var context: ControlCenterContext
    @State private var includeOsUpdates = false
    @State private var failedExternalSparkleStep: CoreUpgradePlanStep?
    @State private var selectedPlanStepIds = Set<String>()
    @State private var knownPlanStepIds = Set<String>()
    private let researchPlanProjection = WholeWorkflowResearchDatasetProvider.activePlanProjection()

    private var researchPlanSteps: [CoreUpgradePlanStep] {
        researchPlanProjection?.steps.map(\.coreUpgradePlanStep) ?? []
    }

    private var planSteps: [CoreUpgradePlanStep] {
        researchPlanProjection == nil ? core.upgradePlanSteps : researchPlanSteps
    }

    private var isResearchPlanActive: Bool {
        WholeWorkflowResearchDatasetProvider.isSelected()
    }

    private var runnableCount: Int {
        selectedScopedPlanSteps.filter(core.upgradePlanStepRunsAutomatically).count
    }

    private var interactiveSparkleCount: Int {
        selectedScopedPlanSteps.filter(HelmCore.isExternalSparklePlanStep).count
    }

    private var scopedPlanSteps: [CoreUpgradePlanStep] {
        HelmCore.scopedUpgradePlanSteps(
            from: planSteps,
            managerScopeId: context.planManagerScopeId,
            packageFilter: context.planPackageFilter
        )
    }

    private var visiblePlanSteps: [CoreUpgradePlanStep] {
        scopedPlanSteps
    }

    private var selectedScopedPlanSteps: [CoreUpgradePlanStep] {
        scopedPlanSteps.filter { selectedPlanStepIds.contains($0.id) }
    }

    private var selectedAutomaticStepIDs: Set<String> {
        automaticallyRunStepIDs(in: selectedScopedPlanSteps)
    }

    private var visiblePlanStepIds: Set<String> {
        Set(visiblePlanSteps.compactMap { step in
            guard researchPlanProjection?.isSelectable(stepID: step.id) ?? true else {
                return nil
            }
            return step.id
        })
    }

    private var visibleSelectionState: UpgradePreviewPlanner.VisibleSelectionState {
        UpgradePreviewPlanner.visibleSelectionState(
            selectedStepIds: selectedPlanStepIds,
            visibleStepIds: visiblePlanStepIds
        )
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

    private var riskSummary: UpgradePreviewPlanner.RiskSummary {
        riskSummary(for: selectedScopedPlanSteps)
    }

    private func riskSummary(
        for selectedSteps: [CoreUpgradePlanStep]
    ) -> UpgradePreviewPlanner.RiskSummary {
        if let researchPlanProjection {
            return researchPlanProjection.riskSummary(
                selectedStepIDs: Set(selectedSteps.map(\.id))
            )
        }
        return UpgradePreviewPlanner.riskSummary(
            for: selectedSteps.map {
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

    private func setPlanStepIncluded(_ stepID: String, included: Bool) {
        guard researchPlanProjection?.isSelectable(stepID: stepID) ?? true else {
            return
        }
        if included {
            selectedPlanStepIds.insert(stepID)
        } else {
            selectedPlanStepIds.remove(stepID)
        }
    }

    private func selectInspectorStep(_ step: CoreUpgradePlanStep) {
        context.selectedUpgradePlanStepId = step.id
        context.selectedTaskId = nil
        context.selectedPackageId = nil
        context.selectedManagerId = nil
    }

    private func reconcilePlanSelection(_ steps: [CoreUpgradePlanStep]) {
        if let researchPlanProjection {
            selectedPlanStepIds = researchPlanProjection.initialSelectedStepIDs
            knownPlanStepIds = Set(researchPlanSteps.map(\.id))
            return
        }
        let selection = UpgradePreviewPlanner.reconcileSelection(
            selectedStepIds: selectedPlanStepIds,
            knownStepIds: knownPlanStepIds,
            availableStepIds: Set(steps.map(\.id))
        )
        selectedPlanStepIds = selection.selectedStepIds
        knownPlanStepIds = selection.knownStepIds
    }

    private func toggleVisiblePlanSelection() {
        selectedPlanStepIds = UpgradePreviewPlanner.settingVisibleSelection(
            selectedStepIds: selectedPlanStepIds,
            visibleStepIds: visiblePlanStepIds,
            included: visibleSelectionState != .all
        )
    }

    private func automaticallyRunStepIDs(
        in selectedSteps: [CoreUpgradePlanStep]
    ) -> Set<String> {
        Set(
            selectedSteps
                .filter(core.upgradePlanStepRunsAutomatically)
                .map(\.id)
        )
    }

    @discardableResult
    private func presentReviewedConfirmation(
        selectedSteps: [CoreUpgradePlanStep]? = nil
    ) -> Bool {
        let selectedSteps = selectedSteps ?? selectedScopedPlanSteps
        let automaticStepIDs = automaticallyRunStepIDs(in: selectedSteps)
        guard ReviewedUpgradePlanPresentationPolicy.canPresent(
            automaticallyRunStepIDs: automaticStepIDs,
            executionAvailable: isResearchPlanActive || core.networkOperationsAvailable
        ) else { return false }
        context.presentReviewedUpgradePlanSheet(
            in: .controlCenter,
            managerScopeID: context.planManagerScopeId,
            packageFilter: context.planPackageFilter,
            selectedSteps: HelmCore.sortedUpgradePlanStepsForExecution(selectedSteps)
                .map { $0.reviewedUpgradePlanStep(status: projectedStatus($0)) },
            automaticallyRunStepIDs: automaticStepIDs,
            riskSummary: riskSummary(for: selectedSteps)
        )
        return true
    }

    private func requestedConfirmationSteps(
        for request: UpgradePlanConfirmationRequest
    ) -> [CoreUpgradePlanStep] {
        let pinnedPackageKeys = Set(core.outdatedPackages.lazy.filter(\.pinned).map {
            "\($0.managerId)\u{0}\($0.name)"
        })
        return planSteps.filter { step in
            guard researchPlanProjection?.isSelectable(stepID: step.id) ?? true else {
                return false
            }
            let isPinned = pinnedPackageKeys.contains("\(step.managerId)\u{0}\(step.packageName)")
            return request.includes(managerID: step.managerId, isPinned: isPinned)
        }
    }

    private func issueRequestedConfirmationPreview(
        for request: UpgradePlanConfirmationRequest
    ) {
        guard let previewRequest = core.refreshUpgradePlan(
            includePinned: request.includePinned,
            allowOsUpdates: request.allowOsUpdates
        ) else { return }
        context.requireUpgradePlanPreview(previewRequest, for: request)
    }

    private func synchronizeRequestedConfirmation(forceNewPreview: Bool = false) {
        guard let request = context.pendingUpgradePlanConfirmationRequest else { return }
        if includeOsUpdates != request.allowOsUpdates {
            includeOsUpdates = request.allowOsUpdates
        }

        if isResearchPlanActive {
            let requestedSteps = requestedConfirmationSteps(for: request)
            selectedPlanStepIds = Set(requestedSteps.map(\.id))
            knownPlanStepIds = Set(planSteps.map(\.id))
            let presentationSucceeded = presentReviewedConfirmation(selectedSteps: requestedSteps)
            context.completeUpgradePlanConfirmationRequest(
                request,
                presentationSucceeded: presentationSucceeded
            )
            return
        }

        if forceNewPreview {
            issueRequestedConfirmationPreview(for: request)
            return
        }
        guard let requiredPreviewRevision = request.requiredPreviewRevision else {
            issueRequestedConfirmationPreview(for: request)
            return
        }
        guard let latestIssuedRequest = core.upgradePlanPreviewRevisionState.latestIssuedRequest else {
            issueRequestedConfirmationPreview(for: request)
            return
        }
        guard latestIssuedRequest.revision == requiredPreviewRevision else {
            if latestIssuedRequest.matches(request) {
                context.requireUpgradePlanPreview(latestIssuedRequest, for: request)
            } else {
                issueRequestedConfirmationPreview(for: request)
            }
            return
        }
        guard UpgradePlanConfirmationPreviewPolicy.isReady(
            request,
            previewState: core.upgradePlanPreviewRevisionState
        ) else {
            return
        }

        let requestedSteps = requestedConfirmationSteps(for: request)
        selectedPlanStepIds = Set(requestedSteps.map(\.id))
        knownPlanStepIds = Set(planSteps.map(\.id))
        let presentationSucceeded = presentReviewedConfirmation(selectedSteps: requestedSteps)
        context.completeUpgradePlanConfirmationRequest(
            request,
            presentationSucceeded: presentationSucceeded
        )
    }

    private func plannerStep(_ step: CoreUpgradePlanStep) -> UpgradePreviewPlanner.PlanStep {
        .init(
            id: step.id,
            orderIndex: step.orderIndex,
            managerId: step.managerId,
            authority: step.authority,
            action: step.action,
            packageName: step.packageName,
            reasonLabelKey: step.reasonLabelKey,
            reasonLabelArgs: step.reasonLabelArgs,
            status: step.status
        )
    }

    private func authorityTitle(for authority: String) -> String {
        let key: String
        switch authority {
        case "authoritative":
            key = L10n.App.Updates.Authority.authoritative
        case "standard":
            key = L10n.App.Updates.Authority.standard
        case "guarded":
            key = L10n.App.Updates.Authority.guarded
        case "detection_only":
            key = L10n.App.Updates.Authority.detectionOnly
        case "interactive":
            key = L10n.App.Updates.Authority.interactive
        default:
            key = L10n.App.Updates.Authority.other
        }
        return key.localized
    }

    private var outlineSections: [UpgradePlanOutlineSection] {
        var sequence = 0
        var stepsByID: [String: CoreUpgradePlanStep] = [:]
        for step in planSteps {
            stepsByID[step.id] = step
        }
        return UpgradePreviewPlanner
            .groupedForPresentation(visiblePlanSteps.map(plannerStep))
            .map { group in
                let rows = group.steps.map { plannerStep in
                    sequence += 1
                    let step = stepsByID[plannerStep.id]
                    let projected = step.map(projectedStatus) ?? plannerStep.status
                    let exclusion = step.flatMap(researchExclusionLabel)
                    let status = exclusion.map {
                        "\(core.localizedUpgradePlanStatus(projected)) (\($0))"
                    } ?? core.localizedUpgradePlanStatus(projected)
                    let tone: UpgradePlanOutlineRow.StatusTone
                    if projected.lowercased() == "failed" {
                        tone = .error
                    } else if exclusion != nil || (step.map(core.upgradePlanStepRunsAutomatically) ?? true) {
                        tone = .standard
                    } else {
                        tone = .needsReview
                    }
                    return UpgradePlanOutlineRow(
                        id: plannerStep.id,
                        sequence: sequence,
                        title: step.map(planStepTitle) ?? plannerStep.packageName,
                        manager: localizedManagerDisplayName(plannerStep.managerId),
                        isIncluded: selectedPlanStepIds.contains(plannerStep.id),
                        isSelectable: researchPlanProjection?.isSelectable(stepID: plannerStep.id) ?? true,
                        status: status,
                        statusTone: tone,
                        actionTitle: step.map(HelmCore.isExternalSparklePlanStep) == true
                            ? L10n.App.Updates.openAppToUpdate.localized
                            : nil
                    )
                }
                return UpgradePlanOutlineSection(
                    id: group.id,
                    title: authorityTitle(for: group.id),
                    summary: L10n.App.Updates.Table.sectionSummary.localized(with: [
                        "count": rows.count
                    ]),
                    rows: rows
                )
            }
    }

    private func performPlanAction(for stepID: String) {
        guard let step = planSteps.first(where: { $0.id == stepID }),
              HelmCore.isExternalSparklePlanStep(step),
              !core.openExternalSparkleApplication(for: step) else {
            return
        }
        failedExternalSparkleStep = step
    }

    private func reconcilePlanScope(_ steps: [CoreUpgradePlanStep]) {
        let managerSet = Set(steps.map(\.managerId))
        if context.planManagerScopeId != HelmCore.allManagersScopeId,
           !managerSet.contains(context.planManagerScopeId) {
            context.planManagerScopeId = HelmCore.allManagersScopeId
        }
    }

    private func projectedStatus(_ step: CoreUpgradePlanStep) -> String {
        if researchPlanProjection != nil {
            return step.status
        }
        return core.projectedUpgradePlanStatus(for: step)
    }

    private func researchExclusionLabel(for step: CoreUpgradePlanStep) -> String? {
        guard let reason = researchPlanProjection?.update(for: step.id)?.exclusionReason else {
            return nil
        }
        switch reason {
        case "pinned":
            return L10n.App.Packages.Label.pinned.localized
        case "operating_system_updates_disabled":
            return L10n.App.Updates.Exclusion.operatingSystemUpdatesDisabled.localized
        default:
            return nil
        }
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
        guard researchPlanProjection == nil else { return [] }
        let scopedSet = Set(scopedPlanSteps.map(\.id))
        return core.upgradePlanFailureGroups.compactMap { group in
            let scopedIds = group.stepIds.filter { scopedSet.contains($0) }
            guard !scopedIds.isEmpty else { return nil }
            let scopedPackages = planSteps
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

    private func reconcileVisibleInspectorSelection() {
        guard let selectedStepID = context.selectedUpgradePlanStepId,
              !visiblePlanSteps.contains(where: { $0.id == selectedStepID }) else {
            return
        }
        context.selectedUpgradePlanStepId = nil
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack {
                Text(ControlCenterSection.updates.title)
                    .font(.title2.weight(.semibold))
                Spacer()
                Button(L10n.App.Action.refreshPlan.localized) {
                    guard !isResearchPlanActive else { return }
                    core.triggerRefresh()
                    core.refreshUpgradePlan(includePinned: false, allowOsUpdates: includeOsUpdates)
                }
                .buttonStyle(HelmSecondaryButtonStyle())
                .disabled(core.isRefreshing || isResearchPlanActive)
            }

            HStack(spacing: 12) {
                Text(L10n.App.Updates.executionPlan.localized)
                    .font(.headline)
                if !core.safeModeEnabled {
                    Toggle(L10n.App.Updates.includeOs.localized, isOn: $includeOsUpdates)
                        .toggleStyle(.switch)
                        .disabled(isResearchPlanActive)
                }
                Spacer()
                if !visiblePlanSteps.isEmpty {
                    Button(action: toggleVisiblePlanSelection) {
                        Label(
                            visibleSelectionState == .all
                                ? L10n.App.Updates.deselectAll.localized
                                : L10n.App.Updates.selectAll.localized,
                            systemImage: visibleSelectionState == .all
                                ? "checkmark.square.fill"
                                : (visibleSelectionState == .partial ? "minus.square.fill" : "square")
                        )
                    }
                    .buttonStyle(HelmSecondaryButtonStyle())
                    .disabled(
                        core.scopedUpgradePlanRunInProgress
                            || visiblePlanStepIds.isEmpty
                    )
                }
            }

            if researchPlanProjection == nil,
               let completion = core.upgradePlanCompletion,
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
                VStack {
                    Spacer()
                    Text(L10n.App.Tasks.noRecentTasks.localized)
                        .font(.subheadline)
                        .foregroundColor(.secondary)
                    Spacer()
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                UpgradePlanOutlineView(
                    sections: outlineSections,
                    selectedStepID: context.selectedUpgradePlanStepId,
                    columnLabels: UpgradePlanOutlineColumnLabels(
                        update: L10n.App.Packages.Action.update.localized,
                        manager: L10n.App.Inspector.manager.localized,
                        included: L10n.App.Updates.Table.included.localized,
                        status: L10n.App.Updates.Table.status.localized,
                        action: L10n.App.Updates.Table.action.localized
                    ),
                    accessibilityLabel: L10n.App.Updates.executionPlan.localized,
                    interactionsEnabled: !core.scopedUpgradePlanRunInProgress,
                    onSelectStep: { stepID in
                        guard let step = planSteps.first(where: { $0.id == stepID }) else { return }
                        selectInspectorStep(step)
                    },
                    onSetIncluded: setPlanStepIncluded,
                    onPerformAction: performPlanAction
                )
                .frame(minHeight: 240, maxHeight: .infinity)
            }

            HStack(spacing: 16) {
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
                Spacer()
            }

            if !scopedFailureGroups.isEmpty {
                VStack(alignment: .leading, spacing: 8) {
                    HStack {
                        Text(L10n.App.Popover.failures.localized)
                            .font(.headline)
                        Spacer()
                        Button(L10n.App.Packages.Action.update.localized) {
                            core.retryUpgradePlanSteps(stepIds: scopedFailedStepIds)
                        }
                        .buttonStyle(HelmSecondaryButtonStyle())
                        .font(.caption)
                        .disabled(scopedFailedStepIds.isEmpty)
                    }

                    ScrollView(.horizontal) {
                        HStack(spacing: 10) {
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
                                .frame(width: 260, alignment: .leading)
                                .helmCardSurface(cornerRadius: 10, highlighted: true)
                            }
                        }
                    }
                    .frame(maxHeight: 104)
                }
            }

            HStack {
                Button(L10n.App.Tasks.Action.cancel.localized) {
                    core.cancelRemainingUpgradePlanSteps(
                        managerScopeId: context.planManagerScopeId,
                        packageFilter: context.planPackageFilter
                    )
                }
                .buttonStyle(HelmSecondaryButtonStyle())
                .disabled(isResearchPlanActive)

                Button(L10n.App.Action.runPlan.localized) {
                    presentReviewedConfirmation()
                }
                .buttonStyle(HelmPrimaryButtonStyle())
                .disabled(
                    runnableCount == 0
                        || core.scopedUpgradePlanRunInProgress
                        || (!isResearchPlanActive && !core.networkOperationsAvailable)
                )

                Spacer()
            }
        }
        .padding(20)
        .onAppear {
            reconcilePlanSelection(planSteps)
            reconcilePlanScope(planSteps)
            if context.pendingUpgradePlanConfirmationRequest != nil {
                synchronizeRequestedConfirmation(forceNewPreview: true)
            } else if !isResearchPlanActive {
                core.refreshUpgradePlan(includePinned: false, allowOsUpdates: includeOsUpdates)
            }
        }
        .onChange(of: includeOsUpdates) { value in
            guard !isResearchPlanActive else { return }
            guard context.pendingUpgradePlanConfirmationRequest == nil else { return }
            core.refreshUpgradePlan(includePinned: false, allowOsUpdates: value)
        }
        .onChange(of: core.safeModeEnabled) { _ in
            guard !isResearchPlanActive else { return }
            core.refreshUpgradePlan(includePinned: false, allowOsUpdates: includeOsUpdates)
        }
        .onChange(of: core.upgradePlanSteps) { steps in
            guard !isResearchPlanActive else { return }
            reconcilePlanSelection(steps)
            reconcilePlanScope(steps)
        }
        .onChange(of: context.pendingUpgradePlanConfirmationRequest) { request in
            guard let request else { return }
            synchronizeRequestedConfirmation(
                forceNewPreview: request.requiredPreviewRevision == nil
            )
        }
        .onChange(of: core.upgradePlanPreviewRevisionState) { _ in
            synchronizeRequestedConfirmation()
        }
        .onChange(of: core.networkOperationsAvailable) { available in
            guard available else { return }
            synchronizeRequestedConfirmation(forceNewPreview: true)
        }
        .onChange(of: core.isConnected) { connected in
            guard connected else { return }
            synchronizeRequestedConfirmation(forceNewPreview: true)
        }
        .onChange(of: context.planManagerScopeId) { _ in
            reconcileVisibleInspectorSelection()
        }
        .onChange(of: context.planPackageFilter) { _ in
            reconcileVisibleInspectorSelection()
        }
        .onChange(of: appUpdate.includeHelmInUpgradeAll) { _ in
            guard !isResearchPlanActive else { return }
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
                .foregroundColor(active ? HelmTheme.stateNeedsReview : HelmTheme.textSecondary)
            Text(flag)
                .font(.subheadline)
                .foregroundColor(active ? HelmTheme.textPrimary : HelmTheme.textSecondary)
        }
    }
}

struct ReviewedUpgradeConfirmationSheet: View {
    private struct ConfirmationRow: Identifiable {
        let id: String
        let sequence: Int
        let step: ReviewedUpgradePlanStep
    }

    private struct AuthorityGroup: Identifiable {
        let id: String
        let rows: [ConfirmationRow]
    }

    let request: ReviewedUpgradePlanRequest
    @ObservedObject private var core = HelmCore.shared
    @EnvironmentObject private var context: ControlCenterContext
    @Environment(\.presentationMode) private var presentationMode
    private let researchPlanProjection = WholeWorkflowResearchDatasetProvider.activePlanProjection()

    private var planSteps: [CoreUpgradePlanStep] {
        researchPlanProjection?.steps.map(\.coreUpgradePlanStep) ?? core.upgradePlanSteps
    }

    private var currentCoreSelectedSteps: [CoreUpgradePlanStep] {
        let scopedSteps = HelmCore.scopedUpgradePlanSteps(
            from: planSteps,
            managerScopeId: request.managerScopeID,
            packageFilter: request.packageFilter
        )
        return HelmCore.sortedUpgradePlanStepsForExecution(
            scopedSteps.filter { request.selectedStepIDs.contains($0.id) }
        )
    }

    private var currentSelectedSteps: [ReviewedUpgradePlanStep] {
        currentCoreSelectedSteps.map {
            $0.reviewedUpgradePlanStep(
                status: researchPlanProjection == nil
                    ? core.projectedUpgradePlanStatus(for: $0)
                    : $0.status
            )
        }
    }

    private var currentAutomaticStepIDs: Set<String> {
        Set(currentCoreSelectedSteps.filter(core.upgradePlanStepRunsAutomatically).map(\.id))
    }

    private var currentRiskSummary: UpgradePreviewPlanner.RiskSummary {
        if let researchPlanProjection {
            return researchPlanProjection.riskSummary(
                selectedStepIDs: Set(currentSelectedSteps.map(\.id))
            )
        }
        return UpgradePreviewPlanner.riskSummary(
            for: currentCoreSelectedSteps.map {
                .init(managerId: $0.managerId, packageName: $0.packageName)
            },
            restartRequiredCandidates: core.outdatedPackages
                .filter(\.restartRequired)
                .map { .init(managerId: $0.managerId, packageName: $0.name) }
        )
    }

    private var confirmationGroups: [AuthorityGroup] {
        let indexedSteps = request.selectedSteps.enumerated().map { index, step in
            ConfirmationRow(id: step.id, sequence: index + 1, step: step)
        }
        return ["authoritative", "standard", "guarded", "detection_only", "interactive", "other"]
            .compactMap { authority in
                let rows = indexedSteps.filter {
                    normalizedAuthority($0.step.authority) == authority
                }
                guard !rows.isEmpty else { return nil }
                return AuthorityGroup(id: authority, rows: rows)
            }
    }

    private var vendorInteractionCount: Int {
        request.selectedSteps.filter(isExternalSparkleStep).count
    }

    private var planIsCurrent: Bool {
        ReviewedUpgradePlanValidation.isCurrent(
            request: request,
            currentSelectedSteps: currentSelectedSteps,
            currentAutomaticallyRunStepIDs: currentAutomaticStepIDs,
            currentRiskSummary: currentRiskSummary
        )
    }

    private var hasRunnableUpdates: Bool {
        !request.automaticallyRunStepIDs.isEmpty && planIsCurrent
    }

    private var isResearchReadOnly: Bool {
        WholeWorkflowResearchDatasetProvider.isSelected()
    }

    private func normalizedAuthority(_ authority: String) -> String {
        switch authority.lowercased() {
        case "authoritative", "standard", "guarded", "detection_only", "interactive":
            return authority.lowercased()
        default:
            return "other"
        }
    }

    private func authorityTitle(for authority: String) -> String {
        switch authority {
        case "authoritative":
            return L10n.App.Updates.Authority.authoritative.localized
        case "standard":
            return L10n.App.Updates.Authority.standard.localized
        case "guarded":
            return L10n.App.Updates.Authority.guarded.localized
        case "detection_only":
            return L10n.App.Updates.Authority.detectionOnly.localized
        case "interactive":
            return L10n.App.Updates.Authority.interactive.localized
        default:
            return L10n.App.Updates.Authority.other.localized
        }
    }

    private func stepTitle(_ step: ReviewedUpgradePlanStep) -> String {
        if step.managerID == "softwareupdate", step.packageName == "__confirm_os_updates__" {
            let arguments = step.reasonLabelArgs.reduce(into: [String: Any]()) { result, entry in
                result[entry.key] = entry.value
            }
            return step.reasonLabelKey.localized(with: arguments)
        }
        return step.packageName
    }

    private func isExternalSparkleStep(_ step: ReviewedUpgradePlanStep) -> Bool {
        step.action == UpgradePreviewPlanner.externalSparkleAction
    }

    private func isHelmSelfUpdateStep(_ step: ReviewedUpgradePlanStep) -> Bool {
        step.action == UpgradePreviewPlanner.helmSelfUpdateAction
            && step.managerID == UpgradePreviewPlanner.helmSelfUpdateManagerId
    }

    private func stepStatus(_ step: ReviewedUpgradePlanStep) -> String {
        if isExternalSparkleStep(step) {
            return L10n.App.Updates.Status.requiresInteraction.localized
        }
        if isHelmSelfUpdateStep(step) {
            return request.automaticallyRunStepIDs.contains(step.id)
                ? L10n.App.Updates.Status.runsLast.localized
                : L10n.App.Updates.Status.notIncluded.localized
        }
        return core.localizedUpgradePlanStatus(step.status)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(L10n.App.Action.runPlan.localized)
                .font(.title3.weight(.semibold))

            Text(
                L10n.App.Updates.Confirmation.selectedSummary.localized(with: [
                    "count": request.selectedSteps.count
                ])
            )
            .font(.callout)
            .foregroundColor(.secondary)

            HStack(spacing: 8) {
                Label(
                    L10n.App.Updates.Confirmation.automaticSummary.localized(with: [
                        "count": request.automaticallyRunStepIDs.count
                    ]),
                    systemImage: "arrow.triangle.2.circlepath"
                )
                .font(.callout.weight(.medium))
                Spacer()
            }

            ScrollView {
                LazyVStack(alignment: .leading, spacing: 12) {
                    ForEach(confirmationGroups) { group in
                        VStack(alignment: .leading, spacing: 6) {
                            HStack {
                                Text(authorityTitle(for: group.id))
                                    .font(.headline)
                                Spacer()
                                Text(
                                    L10n.App.Updates.Table.sectionSummary.localized(with: [
                                        "count": group.rows.count
                                    ])
                                )
                                .font(.caption.monospacedDigit())
                                .foregroundColor(.secondary)
                            }

                            ForEach(group.rows) { row in
                                HStack(alignment: .top, spacing: 10) {
                                    Text("\(row.sequence)")
                                        .font(.caption.monospacedDigit().weight(.semibold))
                                        .foregroundColor(.secondary)
                                        .frame(width: 20, alignment: .trailing)

                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(stepTitle(row.step))
                                            .font(.callout.weight(.medium))
                                            .lineLimit(2)
                                        Text(localizedManagerDisplayName(row.step.managerID))
                                            .font(.caption)
                                            .foregroundColor(.secondary)
                                    }

                                    Spacer(minLength: 12)

                                    Text(stepStatus(row.step))
                                        .font(.caption.weight(.medium))
                                        .foregroundColor(
                                            isExternalSparkleStep(row.step)
                                                ? HelmTheme.stateNeedsReview
                                                : .secondary
                                        )
                                        .multilineTextAlignment(.trailing)
                                }
                                .padding(.horizontal, 10)
                                .padding(.vertical, 8)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .helmCardSurface(cornerRadius: 9)
                                .accessibilityElement(children: .combine)
                            }
                        }
                    }
                }
                .padding(1)
            }
            .frame(minHeight: 170, idealHeight: 260, maxHeight: 320)

            VStack(alignment: .leading, spacing: 6) {
                confirmationRiskRow(
                    label: L10n.App.Updates.Risk.privileged.localized,
                    active: request.riskSummary.requiresElevatedPrivileges
                )
                confirmationRiskRow(
                    label: L10n.App.Updates.Risk.reboot.localized,
                    active: request.riskSummary.mayRequireReboot
                )
            }

            if vendorInteractionCount > 0 {
                Label(
                    L10n.App.Updates.interactiveSparkleNotice.localized(with: [
                        "count": vendorInteractionCount
                    ]),
                    systemImage: "hand.raised.fill"
                )
                .font(.callout)
                .foregroundColor(.secondary)
            }

            if !planIsCurrent {
                Label(
                    L10n.App.Updates.Confirmation.planChanged.localized,
                    systemImage: "exclamationmark.triangle.fill"
                )
                .font(.callout)
                .foregroundColor(HelmTheme.stateNeedsReview)
            }

            if isResearchReadOnly {
                Label(
                    L10n.App.Settings.Alert.UpgradeAll.dryRunToggle.localized,
                    systemImage: "lock.fill"
                )
                .font(.callout)
                .foregroundColor(.secondary)
            }

            Divider()

            HStack {
                Button(L10n.Common.cancel.localized) {
                    context.dismissUpgradeSheet()
                    presentationMode.wrappedValue.dismiss()
                }
                .buttonStyle(HelmSecondaryButtonStyle())
                .keyboardShortcut(.cancelAction)
                Spacer()
                Button(L10n.App.Action.runPlan.localized) {
                    guard !isResearchReadOnly, planIsCurrent else { return }
                    core.runUpgradePlanScoped(
                        managerScopeId: request.managerScopeID,
                        packageFilter: request.packageFilter,
                        selectedStepIds: request.selectedStepIDs
                    )
                    context.dismissUpgradeSheet()
                    presentationMode.wrappedValue.dismiss()
                }
                .buttonStyle(HelmPrimaryButtonStyle())
                .keyboardShortcut(.defaultAction)
                .disabled(
                    !hasRunnableUpdates
                        || isResearchReadOnly
                        || !core.networkOperationsAvailable
                        || core.scopedUpgradePlanRunInProgress
                )
            }
        }
        .padding(20)
        .frame(minWidth: 520, idealWidth: 560, maxWidth: 620)
    }

    private func confirmationRiskRow(label: String, active: Bool) -> some View {
        Label(label, systemImage: active ? "checkmark.circle.fill" : "circle")
            .font(.callout)
            .foregroundColor(active ? HelmTheme.stateNeedsReview : .secondary)
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
            HStack(alignment: .top, spacing: 10) {
                Text(title)
                    .font(.headline)
                    .lineLimit(2, reservesSpace: true)
                    .frame(maxWidth: .infinity, alignment: .topLeading)
                HealthBadgeView(status: status)
                    .fixedSize()
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
            .lineLimit(1)
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
