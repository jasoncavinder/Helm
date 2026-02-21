import SwiftUI
import AppKit

struct RedesignPopoverView: View {
    @ObservedObject private var core = HelmCore.shared
    @ObservedObject private var walkthrough = WalkthroughManager.shared
    @EnvironmentObject private var context: ControlCenterContext
    @Environment(\.colorScheme) private var colorScheme
    @Environment(\.accessibilityReduceMotion) private var accessibilityReduceMotion
    @State private var popoverSearchQuery: String = ""
    @State private var expandedRunningTaskId: String?
    @State private var activeOverlay: PopoverOverlayRoute?
    let onOpenControlCenter: () -> Void

    private var managerRows: [ManagerInfo] {
        core.visibleManagers
            .sorted { lhs, rhs in
                let leftOutdated = core.outdatedCount(forManagerId: lhs.id)
                let rightOutdated = core.outdatedCount(forManagerId: rhs.id)
                if leftOutdated == rightOutdated {
                    return lhs.displayName.localizedCaseInsensitiveCompare(rhs.displayName) == .orderedAscending
                }
                return leftOutdated > rightOutdated
            }
    }

    private var searchResults: [ConsolidatedPackageItem] {
        let query = popoverSearchQuery.trimmingCharacters(in: .whitespacesAndNewlines)
        let results = core.filteredPackages(
            query: query,
            managerId: nil,
            statusFilter: nil
        )
        let limit = query.isEmpty ? 10 : 18
        return Array(results.prefix(limit))
    }

    private var popoverTasks: [TaskItem] {
        Array(core.activeTasks.filter(\.isRunning).prefix(4))
    }

    private var overlayTransition: AnyTransition {
        if accessibilityReduceMotion {
            return .opacity
        }
        return .asymmetric(
            insertion: .move(edge: .bottom).combined(with: .opacity),
            removal: .opacity
        )
    }

    var body: some View {
        ZStack {
            if !core.hasCompletedOnboarding {
                OnboardingContainerView {
                    core.completeOnboarding()
                    core.triggerRefresh()
                    if !walkthrough.hasCompletedPopoverWalkthrough {
                        DispatchQueue.main.asyncAfter(deadline: .now() + 0.6) {
                            walkthrough.startPopoverWalkthrough()
                        }
                    }
                }
            } else {
                ZStack {
                    popoverBaseContent
                        .overlay(
                            Group {
                                if activeOverlay != nil {
                                    Color.black.opacity(colorScheme == .dark ? 0.34 : 0.18)
                                        .ignoresSafeArea()
                                        .transition(.opacity)
                                }
                            },
                            alignment: .center
                        )
                        .blur(radius: activeOverlay == nil || accessibilityReduceMotion ? 0 : 0.8)
                        .allowsHitTesting(activeOverlay == nil)

                    if let activeOverlay {
                        popoverOverlayView(for: activeOverlay)
                            .transition(overlayTransition)
                    }
                }
                .animation(
                    accessibilityReduceMotion
                        ? .easeOut(duration: 0.14)
                        : .spring(response: 0.24, dampingFraction: 0.88),
                    value: activeOverlay
                )
                .onAppear {
                    popoverSearchQuery = context.searchQuery
                    if core.hasCompletedOnboarding {
                        core.triggerRefresh()
                    }
                }
                .onChange(of: context.popoverOverlayRequest) { route in
                    guard let route else { return }
                    activeOverlay = route
                }
                .onChange(of: context.popoverOverlayDismissToken) { _ in
                    if activeOverlay != nil {
                        closeOverlay()
                    }
                }
                .onChange(of: context.popoverSearchFocusToken) { _ in
                    activeOverlay = .search
                }
                .onChange(of: activeOverlay) { route in
                    context.isPopoverOverlayVisible = route != nil
                    if route != nil {
                        NSCursor.arrow.set()
                    }
                }
                .onChange(of: popoverTasks.map { "\($0.id):\($0.status)" }) { _ in
                    collapseExpandedTaskIfNeeded()
                }
            }
        }
        .sheet(isPresented: $context.showUpgradeSheet) {
            RedesignUpgradeSheetView()
                .environmentObject(context)
        }
        .overlayPreferenceValue(SpotlightAnchorKey.self) { anchors in
            if walkthrough.isPopoverWalkthroughActive {
                SpotlightOverlay(manager: walkthrough, anchors: anchors)
            }
        }
    }

    private var popoverBaseContent: some View {
        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 12) {
                if !core.isConnected || core.failedTaskCount > 0 || !core.outdatedPackages.isEmpty {
                    PopoverAttentionBanner(onOpenControlCenter: {
                        onOpenControlCenter()
                    })
                        .spotlightAnchor("attentionBanner")
                }

                PopoverSearchField(
                    popoverSearchQuery: $popoverSearchQuery,
                    onSyncSearchQuery: syncSearchQuery,
                    onActivateSearch: {
                        activeOverlay = .search
                    }
                )
                .spotlightAnchor("searchField")

                HStack(alignment: .top) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(L10n.App.Dashboard.title.localized)
                            .font(.headline.weight(.semibold))
                        Text(L10n.App.Popover.systemHealth.localized)
                            .font(.caption)
                            .foregroundColor(.secondary)
                    }
                    Spacer()
                    HealthBadgeView(status: core.aggregateHealth)
                }
                .spotlightAnchor("healthBadge")
                .padding(.top, 4)

                HStack(spacing: 8) {
                    MetricChipView(
                        label: L10n.App.Popover.pendingUpdates.localized,
                        value: core.outdatedPackages.count
                    )
                    MetricChipView(
                        label: L10n.App.Popover.failures.localized,
                        value: core.failedTaskCount
                    )
                    MetricChipView(
                        label: L10n.App.Popover.runningTasks.localized,
                        value: core.runningTaskCount
                    )
                }

                managerSnapshotCard
                    .spotlightAnchor("managerSnapshot")
                tasksCard
                    .spotlightAnchor("activeTasks")
            }
            .padding(16)

            Divider()

            popoverFooter
                .padding(.horizontal, 14)
                .padding(.vertical, 10)
                .spotlightAnchor("footerActions")
        }
        .frame(width: 400)
        .background(
            LinearGradient(
                colors: colorScheme == .dark
                    ? [
                        HelmTheme.surfaceBase.opacity(0.9),
                        HelmTheme.surfaceElevated.opacity(0.82)
                    ]
                    : [
                        Color.white.opacity(0.98),
                        HelmTheme.surfacePanel.opacity(0.86)
                    ],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )
        )
    }

    private var managerSnapshotCard: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text(L10n.App.Popover.managerSnapshot.localized)
                    .font(.subheadline.weight(.semibold))
                Spacer()
                Button(L10n.App.Action.openControlCenter.localized) {
                    context.selectedSection = .managers
                    onOpenControlCenter()
                }
                .buttonStyle(.plain)
                .font(.caption.weight(.semibold))
                .foregroundColor(.secondary)
                .helmPointer()
            }

            if managerRows.isEmpty {
                Text(L10n.App.Dashboard.State.emptyManagers.localized)
                    .font(.caption)
                    .foregroundColor(.secondary)
            } else {
                ForEach(managerRows.prefix(4)) { manager in
                    Button {
                        context.selectedManagerId = manager.id
                        context.selectedPackageId = nil
                        context.selectedTaskId = nil
                        context.selectedUpgradePlanStepId = nil
                        context.selectedSection = .managers
                        onOpenControlCenter()
                    } label: {
                        HStack(spacing: 8) {
                            Text(localizedManagerDisplayName(manager.id))
                                .font(.caption)
                                .lineLimit(1)
                            Spacer()
                            Text("\(core.outdatedCount(forManagerId: manager.id))")
                                .font(.caption.monospacedDigit())
                                .foregroundColor(.secondary)
                            HealthBadgeView(status: core.health(forManagerId: manager.id))
                        }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .helmPointer()
                    .accessibilityElement(children: .combine)
                    .accessibilityLabel(localizedManagerDisplayName(manager.id))
                    .accessibilityValue("\(core.outdatedCount(forManagerId: manager.id)) \(L10n.App.Packages.Filter.upgradable.localized)")
                }
            }
        }
        .padding(12)
        .background(cardBackground)
    }

    private var tasksCard: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text(L10n.App.Popover.activeTasks.localized)
                    .font(.subheadline.weight(.semibold))
                Spacer()
                Button {
                    context.selectedSection = .tasks
                    onOpenControlCenter()
                } label: {
                    Image(systemName: "arrow.right")
                        .font(.caption)
                }
                .buttonStyle(.plain)
                .foregroundColor(.secondary)
                .helmPointer()
                .accessibilityLabel(L10n.App.Action.openControlCenter.localized)
            }

            if popoverTasks.isEmpty {
                Text(L10n.App.Tasks.noRecentTasks.localized)
                    .font(.caption)
                    .foregroundColor(.secondary)
            } else {
                ForEach(popoverTasks) { task in
                    TaskRowView(
                        task: task,
                        onCancel: task.isRunning ? { core.cancelTask(task) } : nil,
                        canExpandDetails: true,
                        isExpanded: expandedRunningTaskId == task.id,
                        onToggleDetails: {
                            if expandedRunningTaskId == task.id {
                                expandedRunningTaskId = nil
                            } else {
                                expandedRunningTaskId = task.id
                            }
                        }
                    )
                }
            }
        }
        .padding(12)
        .background(cardBackground)
    }

    private var popoverFooter: some View {
        HStack(spacing: 10) {
            Button(L10n.App.Popover.version.localized(with: ["version": helmVersion])) {
                activeOverlay = .about
            }
            .buttonStyle(.plain)
            .font(.caption2)
            .foregroundColor(.secondary)
            .helmPointer()

            Spacer(minLength: 10)

            footerIconButton(symbol: "gearshape", accessibilityText: L10n.Common.settings.localized, action: {
                activeOverlay = .quickSettings
            })

            footerIconButton(symbol: "power", accessibilityText: L10n.App.Settings.Action.quit.localized, action: {
                activeOverlay = .confirmQuit
            })
        }
    }

    @ViewBuilder
    private func popoverOverlayView(for route: PopoverOverlayRoute) -> some View {
        switch route {
        case .search:
            PopoverOverlayCard(
                title: L10n.App.Overlay.Search.title.localized,
                onClose: closeOverlay
            ) {
                PopoverSearchOverlayContent(
                    popoverSearchQuery: $popoverSearchQuery,
                    searchResults: searchResults,
                    onSyncSearchQuery: syncSearchQuery,
                    onOpenControlCenter: onOpenControlCenter,
                    onClose: closeOverlay
                )
            }
        case .quickSettings:
            PopoverOverlayCard(
                title: L10n.App.Overlay.Settings.title.localized,
                onClose: closeOverlay
            ) {
                PopoverSettingsOverlayContent(
                    onOpenControlCenter: onOpenControlCenter,
                    onClose: closeOverlay
                )
            }
        case .about:
            PopoverOverlayCard(
                title: L10n.App.Overlay.About.title.localized,
                onClose: closeOverlay
            ) {
                PopoverAboutOverlayContent(onClose: closeOverlay)
            }
        case .confirmQuit:
            PopoverOverlayCard(
                title: L10n.App.Overlay.Quit.title.localized,
                onClose: closeOverlay
            ) {
                PopoverQuitOverlayContent(onClose: closeOverlay)
            }
        }
    }

    private var cardBackground: some View {
        RoundedRectangle(cornerRadius: 12, style: .continuous)
            .fill(HelmTheme.surfacePanel)
            .overlay(
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .strokeBorder(HelmTheme.borderSubtle.opacity(0.95), lineWidth: 0.8)
            )
    }

    private func closeOverlay() {
        activeOverlay = nil
        context.popoverOverlayRequest = nil
    }

    private func collapseExpandedTaskIfNeeded() {
        guard let expandedRunningTaskId else { return }
        let stillRunning = popoverTasks.contains {
            $0.id == expandedRunningTaskId && $0.isRunning
        }
        if !stillRunning {
            self.expandedRunningTaskId = nil
        }
    }

    private func syncSearchQuery(_ query: String) {
        context.searchQuery = query
        core.searchText = query
        if query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            if activeOverlay == .search {
                activeOverlay = nil
            }
        } else if activeOverlay == nil || activeOverlay == .search {
            activeOverlay = .search
        }
    }

    @ViewBuilder
    private func footerIconButton(symbol: String, accessibilityText: String? = nil, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 11, weight: .semibold))
                .frame(width: 24, height: 24)
                .background(
                    Circle()
                        .fill(HelmTheme.surfaceElevated)
                        .overlay(
                            Circle()
                                .strokeBorder(HelmTheme.borderSubtle.opacity(0.85), lineWidth: 0.8)
                        )
                )
        }
        .buttonStyle(.plain)
        .helmPointer()
        .accessibilityLabel(accessibilityText ?? symbol)
    }
}
