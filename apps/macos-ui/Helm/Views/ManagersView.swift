import SwiftUI
import UniformTypeIdentifiers

private struct ManagerDependencyAlertState: Identifiable {
    enum Kind {
        case disableBlocked(managerId: String, dependents: [String])
        case enableRequiresParent(managerId: String, parentManagerId: String)
    }

    let id = UUID()
    let kind: Kind
}

private struct ResearchEnvironmentManagerItem: Identifiable {
    let manager: ManagerInfo
    let record: ResearchManagerRecord

    var id: String { record.id }
}

struct ManagersSectionView: View {
    private let core = HelmCore.shared
    private let researchEnvironmentState = WholeWorkflowResearchDatasetProvider
        .environmentRuntimeState()
    @ObservedObject private var managersState = HelmCore.shared.managersState
    @EnvironmentObject private var context: ControlCenterContext
    @State private var draggedManagerId: String?
    @State private var managerDependencyAlert: ManagerDependencyAlertState?

    private var researchEnvironmentProjection: ResearchEnvironmentProjection? {
        guard case let .ready(projection) = researchEnvironmentState else { return nil }
        return projection
    }

    private var groupedManagers: [(authority: ManagerAuthority, managers: [ManagerInfo])] {
        [
            (
                authority: .authoritative,
                managers: routeFiltered(managersState.authoritativeManagers)
            ),
            (authority: .standard, managers: routeFiltered(managersState.standardManagers)),
            (authority: .guarded, managers: routeFiltered(managersState.guardedManagers))
        ]
    }

    private var hasImplementedManagers: Bool {
        groupedManagers.contains { !$0.managers.isEmpty }
    }

    private var researchGroupedManagers: [(
        authority: ManagerAuthority,
        managers: [ResearchEnvironmentManagerItem]
    )] {
        guard let researchEnvironmentProjection else { return [] }
        let items = researchEnvironmentProjection.managers.compactMap { record in
            ManagerInfo.all.first(where: { $0.id == record.id }).map {
                ResearchEnvironmentManagerItem(manager: $0, record: record)
            }
        }
        return ManagerAuthority.allCases.map { authority in
            (
                authority: authority,
                managers: routeFiltered(
                    items.filter { $0.record.authority == researchAuthorityID(authority) }
                )
            )
        }
    }

    private func researchAuthorityID(_ authority: ManagerAuthority) -> String {
        switch authority {
        case .authoritative:
            return "authoritative"
        case .standard:
            return "standard"
        case .guarded:
            return "guarded"
        }
    }

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 12) {
                HStack(spacing: 12) {
                    Text(ControlCenterSection.managers.title)
                        .font(.title2.weight(.semibold))

                    Spacer()

                    if let routeStage = context.environmentRouteStage {
                        Button {
                            context.clearEnvironmentRouteStage()
                        } label: {
                            HStack(spacing: 6) {
                                Image(systemName: routeStage.symbol)
                                Text(routeStage.titleKey.localized)
                                Image(systemName: "xmark.circle.fill")
                                    .foregroundColor(HelmTheme.textSecondary)
                            }
                            .font(.caption.weight(.semibold))
                            .padding(.horizontal, 10)
                            .frame(height: 28)
                            .background(HelmTheme.selectionFill, in: Capsule())
                        }
                        .buttonStyle(.plain)
                        .helmPointer()
                        .help(L10n.App.Packages.Filter.allManagers.localized)
                        .accessibilityLabel(routeStage.titleKey.localized)
                        .accessibilityHint(L10n.App.Packages.Filter.allManagers.localized)
                    }
                }
                .padding(.horizontal, 20)
                .padding(.top, 20)

                switch researchEnvironmentState {
                case let .ready(projection):
                    researchManagerGroups(projection)
                case .unavailable:
                    researchUnavailableState
                case .inactive:
                    productionManagerGroups
                }
            }
            .padding(.bottom, 18)
        }
        .onDisappear {
            draggedManagerId = nil
        }
        .alert(item: $managerDependencyAlert) { alertState in
            switch alertState.kind {
            case let .disableBlocked(managerId, dependents):
                return Alert(
                    title: Text(
                        L10n.App.Managers.Alert.disableBlockedTitle.localized(
                            with: ["manager": localizedManagerDisplayName(managerId)]
                        )
                    ),
                    message: Text(
                        L10n.App.Managers.Alert.disableBlockedMessage.localized(
                            with: [
                                "manager": localizedManagerDisplayName(managerId),
                                "dependents": localizedDependentManagerList(dependents)
                            ]
                        )
                    ),
                    dismissButton: .default(Text(L10n.Common.ok.localized))
                )
            case let .enableRequiresParent(managerId, parentManagerId):
                return Alert(
                    title: Text(
                        L10n.App.Managers.Alert.enableRequiresParentTitle.localized(
                            with: ["manager": localizedManagerDisplayName(managerId)]
                        )
                    ),
                    message: Text(
                        L10n.App.Managers.Alert.enableRequiresParentMessage.localized(
                            with: [
                                "manager": localizedManagerDisplayName(managerId),
                                "parent": localizedManagerDisplayName(parentManagerId)
                            ]
                        )
                    ),
                    primaryButton: .default(Text(L10n.Common.continue.localized)) {
                        core.setManagerEnabled(parentManagerId, enabled: true) { success in
                            guard success else { return }
                            core.setManagerEnabled(managerId, enabled: true)
                        }
                    },
                    secondaryButton: .cancel(Text(L10n.Common.cancel.localized))
                )
            }
        }
    }

    private func routeFiltered(_ managers: [ManagerInfo]) -> [ManagerInfo] {
        guard let routeStage = context.environmentRouteStage else { return managers }
        return managers.filter {
            WayfinderPopoverRouteStage.stage(forManagerCategory: $0.category) == routeStage
        }
    }

    private func routeFiltered(
        _ managers: [ResearchEnvironmentManagerItem]
    ) -> [ResearchEnvironmentManagerItem] {
        guard let routeStage = context.environmentRouteStage else { return managers }
        return managers.filter {
            WayfinderPopoverRouteStage.stage(forManagerCategory: $0.manager.category) == routeStage
        }
    }

    @ViewBuilder
    private func researchManagerGroups(
        _ projection: ResearchEnvironmentProjection
    ) -> some View {
        let decisionState = context.researchManagerDecisionState(for: projection.decision)
        ForEach(researchGroupedManagers, id: \.authority) { group in
            if !group.managers.isEmpty {
                VStack(alignment: .leading, spacing: 8) {
                    managerGroupHeading(group.authority)
                    ForEach(group.managers) { item in
                        ResearchEnvironmentManagerRow(
                            item: item,
                            decisionState: decisionState,
                            isDecisionTarget: item.id == projection.targetManagerID,
                            isSelected: context.selectedManagerId == item.id
                        ) {
                            context.selectedManagerId = item.id
                            context.selectedPackageId = nil
                            context.selectedTaskId = nil
                            context.selectedUpgradePlanStepId = nil
                        }
                    }
                }
            }
        }
    }

    private var researchUnavailableState: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label(
                L10n.App.Health.unavailable.localized,
                systemImage: OperationalHealth.unavailable.icon
            )
            .font(.headline)
            .foregroundColor(HelmTheme.stateUnavailable)

            Text(L10n.App.Managers.Research.datasetUnavailable.localized)
                .font(.callout)
                .foregroundColor(.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .helmCardSurface(cornerRadius: 12)
        .padding(.horizontal, 20)
        .accessibilityElement(children: .combine)
    }

    @ViewBuilder
    private var productionManagerGroups: some View {
        ForEach(groupedManagers, id: \.authority) { group in
            if !group.managers.isEmpty {
                VStack(alignment: .leading, spacing: 8) {
                    managerGroupHeading(group.authority)
                    ForEach(group.managers) { manager in
                        productionManagerRow(manager, authority: group.authority)
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

    private func managerGroupHeading(_ authority: ManagerAuthority) -> some View {
        Text(authority.key.localized)
            .font(.caption.weight(.semibold))
            .foregroundColor(.secondary)
            .textCase(.uppercase)
            .padding(.horizontal, 20)
    }

    private func productionManagerRow(
        _ manager: ManagerInfo,
        authority: ManagerAuthority
    ) -> some View {
        let canReorder = ManagerPriorityDragPolicy.canInitiateDrag(
            isDetected: core.isManagerDetected(manager.id)
        )
        return ManagerSectionRow(
            manager: manager,
            status: managersState.managerStatusesById[manager.id],
            health: core.health(forManagerId: manager.id),
            outdatedCount: managersState.outdatedCountByManager[manager.id, default: 0],
            packageCount: managersState.installedCountByManager[manager.id, default: 0],
            operationStatus: managersState.managerOperationsById[manager.id],
            isManagerUninstalling: core.isManagerUninstalling(manager.id),
            isSelected: context.selectedManagerId == manager.id,
            canReorder: canReorder,
            onSelect: {
                context.selectedManagerId = manager.id
                context.selectedPackageId = nil
                context.selectedTaskId = nil
                context.selectedUpgradePlanStepId = nil
            },
            onViewPackages: {
                context.selectedManagerId = manager.id
                context.selectedPackageId = nil
                context.selectedTaskId = nil
                context.selectedUpgradePlanStepId = nil
                context.managerFilterId = manager.id
                context.selectedSection = .packages
            },
            onDetectManager: {
                context.selectedManagerId = manager.id
                context.selectedPackageId = nil
                context.selectedTaskId = nil
                context.selectedUpgradePlanStepId = nil
                context.selectedSection = .managers
                core.triggerDetection(for: manager.id)
            },
            onInstallManager: {
                context.selectedManagerId = manager.id
                context.selectedPackageId = nil
                context.selectedTaskId = nil
                context.selectedUpgradePlanStepId = nil
                context.selectedSection = .managers
                context.requestManagerInstallSheet(for: manager.id)
            },
            onToggleEnabled: { enabled in
                handleManagerToggle(managerId: manager.id, enable: enabled)
            }
        )
        .modifier(
            ManagerPriorityDragModifier(
                managerId: manager.id,
                canInitiateDrag: canReorder,
                draggedManagerId: $draggedManagerId
            )
        )
        .onDrop(
            of: [UTType.text.identifier],
            delegate: ManagerPriorityDropDelegate(
                core: core,
                authority: authority,
                targetManagerId: manager.id,
                draggedManagerId: $draggedManagerId
            )
        )
    }

    private func handleManagerToggle(managerId: String, enable: Bool) {
        guard let status = managersState.managerStatusesById[managerId] else {
            core.setManagerEnabled(managerId, enabled: enable)
            return
        }

        if !enable {
            let dependents = ManagerDependencyResolver.enabledDependents(
                of: managerId,
                statuses: managersState.managerStatusesById
            )
            if !dependents.isEmpty {
                managerDependencyAlert = .init(
                    kind: .disableBlocked(managerId: managerId, dependents: dependents)
                )
                return
            }
            core.setManagerEnabled(managerId, enabled: false)
            return
        }

        if let parentManagerId = ManagerDependencyResolver.dependencyManagerId(
            for: managerId,
            provenance: status.activeProvenance
        ),
            let parentStatus = managersState.managerStatusesById[parentManagerId],
            !parentStatus.enabled
        {
            managerDependencyAlert = .init(
                kind: .enableRequiresParent(
                    managerId: managerId,
                    parentManagerId: parentManagerId
                )
            )
            return
        }

        core.setManagerEnabled(managerId, enabled: true)
    }

    private func localizedDependentManagerList(_ managerIds: [String]) -> String {
        managerIds.map(localizedManagerDisplayName).joined(separator: ", ")
    }
}

private struct ResearchEnvironmentManagerRow: View {
    let item: ResearchEnvironmentManagerItem
    let decisionState: ResearchManagerDecisionState
    let isDecisionTarget: Bool
    let isSelected: Bool
    let onSelect: () -> Void

    private var health: OperationalHealth {
        OperationalHealth(
            researchState: ResearchManagerHealthPolicy.health(
                for: item.record,
                decisionState: decisionState,
                isDecisionTarget: isDecisionTarget
            )
        )
    }

    private var findingSummary: String? {
        if isDecisionTarget {
            return decisionState == .acknowledged
                ? L10n.App.Inspector.MultiInstance.acknowledgedTitle.localized
                : L10n.App.Inspector.MultiInstance.attentionTitle.localized
        }
        if item.record.findingCode == "source_refresh_failed" {
            return L10n.App.Managers.Research.sourceRefreshFailed.localized
        }
        return item.record.findingCode == nil ? nil : L10n.App.Health.needsReview.localized
    }

    private var findingSymbol: String {
        if decisionState == .acknowledged, isDecisionTarget {
            return "checkmark.seal.fill"
        }
        return health.icon
    }

    private var findingColor: Color {
        decisionState == .acknowledged && isDecisionTarget
            ? HelmTheme.stateHealthy
            : health.color
    }

    private var freshnessLabel: String {
        switch item.record.freshness {
        case "current":
            return L10n.App.Managers.Research.current.localized
        case "cached":
            return L10n.App.Managers.Research.cached.localized
        default:
            return L10n.App.Managers.Research.unknown.localized
        }
    }

    private var enabledLabel: String {
        guard item.record.detected else { return L10n.App.Managers.State.notInstalled.localized }
        return item.record.enabled
            ? L10n.App.Managers.State.enabled.localized
            : L10n.App.Managers.State.disabled.localized
    }

    var body: some View {
        Button(action: onSelect) {
            HStack(alignment: .top, spacing: 10) {
                HealthBadgeView(status: health)

                VStack(alignment: .leading, spacing: 4) {
                    Text(localizedManagerDisplayName(item.manager.id))
                        .font(.body.weight(.medium))
                    HStack(spacing: 6) {
                        Text(
                            L10n.App.Managers.Research.installInstanceCount.localized(
                                with: ["count": item.record.installInstances.count]
                            )
                        )
                        Text("|")
                        Text(freshnessLabel)
                    }
                    .font(.caption)
                    .foregroundColor(.secondary)

                    if let findingSummary {
                        Label(findingSummary, systemImage: findingSymbol)
                            .font(.caption2)
                            .foregroundColor(findingColor)
                            .lineLimit(2)
                    }
                }

                Spacer(minLength: 12)

                Text(enabledLabel)
                    .font(.caption)
                    .foregroundColor(.secondary)
            }
            .padding(12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .helmCardSurface(cornerRadius: 12)
            .overlay(
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .fill(
                        isSelected
                            ? HelmTheme.selectionFill
                            : (
                                health == .needsReview
                                    ? HelmTheme.stateNeedsReview.opacity(0.045)
                                    : Color.clear
                            )
                    )
                    .overlay(
                        RoundedRectangle(cornerRadius: 12, style: .continuous)
                            .strokeBorder(
                                isSelected
                                    ? HelmTheme.selectionStroke
                                    : (
                                        health == .needsReview
                                            ? HelmTheme.stateNeedsReview.opacity(0.48)
                                            : Color.clear
                                    ),
                                lineWidth: 0.9
                            )
                    )
                    .allowsHitTesting(false)
            )
        }
        .buttonStyle(.plain)
        .padding(.horizontal, 20)
        .helmPointer()
        .accessibilityLabel(localizedManagerDisplayName(item.manager.id))
        .accessibilityValue(
            [health.key.localized, findingSummary, enabledLabel, freshnessLabel]
                .compactMap { $0 }
                .joined(separator: ", ")
        )
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
    let canReorder: Bool
    let onSelect: () -> Void
    let onViewPackages: () -> Void
    let onDetectManager: () -> Void
    let onInstallManager: () -> Void
    let onToggleEnabled: (Bool) -> Void

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

    private var packageActionEnabled: Bool {
        packageCount > 0 && enabled && !isManagerUninstalling
    }

    private var installMethodPolicyContext: ManagerInstallMethodPolicyContext {
        ManagerInstallMethodPolicyContext.fromEnvironment()
    }

    private var resolvedInstallMethodOptions: [ManagerInstallMethodOption] {
        guard let coreOptions = status?.installMethodOptions,
              !coreOptions.isEmpty else {
            return manager.installMethodOptions
        }

        let fallbackByMethod = Dictionary(
            uniqueKeysWithValues: manager.installMethodOptions.map { ($0.method.rawValue, $0) }
        )
        let mapped = coreOptions.compactMap { option in
            ManagerInstallMethodOption.fromCoreStatus(
                option,
                fallback: fallbackByMethod[option.methodId]
            )
        }
        return mapped.isEmpty ? manager.installMethodOptions : mapped
    }

    private var managerCanInstall: Bool {
        let supportsHelmInstall = Set([
            "mise",
            "asdf",
            "mas",
            "rustup",
            "npm",
            "pnpm",
            "yarn",
            "pipx",
            "pip",
            "poetry",
            "rubygems",
            "bundler",
            "cargo",
            "cargo_binstall",
            "podman",
            "colima"
        ]).contains(manager.id)
        guard supportsHelmInstall, !manager.isDetectionOnly else { return false }
        let allowedInstallOptions = resolvedInstallMethodOptions.filter { option in
            option.method != .notManageable && option.isAllowed(in: installMethodPolicyContext)
        }
        if !allowedInstallOptions.isEmpty {
            return true
        }
        return manager.canInstall
    }

    private var reviewFindingSummary: String? {
        if status?.multiInstanceState == "attention_needed" {
            return L10n.App.Inspector.MultiInstance.attentionTitle.localized
        }
        if status?.packageStateIssues?.contains(where: { issue in
            issue.issueCode == "post_install_setup_required"
        }) == true {
            return "app.inspector.package_state_issue.setup_required.title".localized
        }
        if let issue = status?.packageStateIssues?.first(where: { issue in
            issue.issueCode == "metadata_only_install"
        }) {
            return L10n.App.Managers.State.metadataMismatch.localized(with: [
                "package": issue.packageName
            ])
        }
        if let issue = status?.packageStateIssues?.first {
            if issue.issueCode == "homebrew_cellar_lock_conflict" {
                return "app.inspector.package_state_issue.homebrew_lock.title".localized
            }
            return issue.summary ?? L10n.App.Health.needsReview.localized
        }
        return ineligibleReason
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 10) {
                Image(systemName: "line.3.horizontal")
                    .font(.caption.weight(.semibold))
                    .foregroundColor(HelmTheme.textSecondary)
                    .opacity(canReorder ? 1 : 0)
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
                            .foregroundColor(
                                outdatedCount == 0
                                    ? HelmTheme.textSecondary
                                    : HelmTheme.stateUpdatesReady
                            )
                    }
                    if let reviewFindingSummary {
                        Label(reviewFindingSummary, systemImage: "exclamationmark.triangle.fill")
                            .font(.caption2)
                            .foregroundColor(HelmTheme.stateNeedsReview)
                            .lineLimit(2)
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
                            onToggleEnabled(!enabled)
                        }
                    ))
                    .toggleStyle(.switch)
                    .labelsHidden()
                    .scaleEffect(0.75)
                    .disabled(enableToggleDisabled || isManagerUninstalling)
                }
            }

            HStack(spacing: 8) {
                if enabled && outdatedCount > 0 {
                    Button(L10n.App.Settings.Action.upgradeAll.localized) {
                        core.upgradeAllPackages(forManagerId: manager.id)
                    }
                    .disabled(isManagerUninstalling || !core.networkOperationsAvailable)
                    .helmPointer(enabled: !isManagerUninstalling && core.networkOperationsAvailable)
                }

                Spacer()

                if detected {
                    managerCardActionButton(
                        symbol: "shippingbox",
                        tooltip: L10n.App.Managers.Action.viewPackages.localized,
                        enabled: packageActionEnabled
                    ) {
                        onViewPackages()
                    }
                } else if managerCanInstall {
                    managerCardActionButton(
                        symbol: "arrow.down.circle",
                        tooltip: L10n.Common.install.localized,
                        enabled: !isManagerUninstalling && core.networkOperationsAvailable
                    ) {
                        onInstallManager()
                    }
                } else {
                    managerCardActionButton(
                        symbol: "scope",
                        tooltip: L10n.Common.detect.localized,
                        enabled: !isManagerUninstalling
                    ) {
                        onDetectManager()
                    }
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
                .fill(
                    isSelected
                        ? HelmTheme.selectionFill
                        : (health == .needsReview ? HelmTheme.stateNeedsReview.opacity(0.045) : Color.clear)
                )
                .overlay(
                    RoundedRectangle(cornerRadius: 12, style: .continuous)
                        .strokeBorder(
                            isSelected
                                ? HelmTheme.selectionStroke
                                : (
                                    health == .needsReview
                                        ? HelmTheme.stateNeedsReview.opacity(0.48)
                                        : Color.clear
                                ),
                            lineWidth: 0.9
                        )
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
            reviewFindingSummary,
            detected ? (enabled ? L10n.App.Managers.State.enabled.localized : L10n.App.Managers.State.disabled.localized) : L10n.App.Managers.State.notInstalled.localized,
            L10n.App.Managers.Label.packageCount.localized(with: ["count": packageCount])
        ].compactMap { $0 }.joined(separator: ", "))
    }

    private func managerCardActionButton(
        symbol: String,
        tooltip: String,
        enabled: Bool,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
        }
        .buttonStyle(HelmIconButtonStyle())
        .help(tooltip)
        .accessibilityLabel(tooltip)
        .disabled(!enabled)
        .helmPointer(enabled: enabled)
    }

}

// Backward compatibility wrapper for legacy references.
struct ManagersView: View {
    @Binding var selectedTab: HelmTab

    var body: some View {
        ManagersSectionView()
    }
}

private struct ManagerPriorityDragModifier: ViewModifier {
    let managerId: String
    let canInitiateDrag: Bool
    @Binding var draggedManagerId: String?

    @ViewBuilder
    func body(content: Content) -> some View {
        if canInitiateDrag {
            content.onDrag {
                draggedManagerId = managerId
                return NSItemProvider(object: managerId as NSString)
            }
        } else {
            content
        }
    }
}

private struct ManagerPriorityDropDelegate: DropDelegate {
    let core: HelmCore
    let authority: ManagerAuthority
    let targetManagerId: String
    @Binding var draggedManagerId: String?

    func performDrop(info: DropInfo) -> Bool {
        guard let draggedManagerId else { return false }
        core.moveManagerPriority(
            authority: authority,
            draggedManagerId: draggedManagerId,
            targetManagerId: targetManagerId
        )
        self.draggedManagerId = nil
        return true
    }

    func dropExited(info: DropInfo) {
        if !info.hasItemsConforming(to: [UTType.text.identifier]) {
            draggedManagerId = nil
        }
    }
}
