import SwiftUI
import AppKit

struct RedesignPopoverView: View {
    @ObservedObject private var core = HelmCore.shared
    @EnvironmentObject private var context: ControlCenterContext
    @ObservedObject private var localization = LocalizationManager.shared
    @Environment(\.colorScheme) private var colorScheme
    @Environment(\.accessibilityReduceMotion) private var accessibilityReduceMotion
    @FocusState private var isOverlaySearchFocused: Bool
    @State private var popoverSearchQuery: String = ""
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

    private var hasAttentionBanner: Bool {
        !core.isConnected || core.failedTaskCount > 0 || core.outdatedPackages.count > 0
    }

    private var searchResults: [PackageItem] {
        let query = popoverSearchQuery.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !query.isEmpty else {
            return Array(core.allKnownPackages.prefix(10))
        }

        let localMatches = core.allKnownPackages.filter {
            $0.name.lowercased().contains(query) || $0.manager.lowercased().contains(query)
        }
        let localIds = Set(localMatches.map(\.id))
        let remoteMatches = core.searchResults.filter { !localIds.contains($0.id) }
        return Array((localMatches + remoteMatches).prefix(18))
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
                }
            } else {
                ZStack {
                    popoverBaseContent
                        .overlay {
                            if activeOverlay != nil {
                                Color.black.opacity(colorScheme == .dark ? 0.34 : 0.18)
                                    .ignoresSafeArea()
                                    .transition(.opacity)
                            }
                        }
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
                    if route == .search {
                        isOverlaySearchFocused = true
                    }
                }
                .onChange(of: context.popoverOverlayDismissToken) { _ in
                    if activeOverlay != nil {
                        closeOverlay()
                    }
                }
                .onChange(of: context.popoverSearchFocusToken) { _ in
                    activeOverlay = .search
                    isOverlaySearchFocused = true
                }
                .onChange(of: activeOverlay) { route in
                    context.isPopoverOverlayVisible = route != nil
                    if route != nil {
                        NSCursor.arrow.set()
                    }
                }
            }
        }
        .sheet(isPresented: $context.showUpgradeSheet) {
            RedesignUpgradeSheetView()
                .environmentObject(context)
        }
    }

    private var popoverBaseContent: some View {
        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 12) {
                if hasAttentionBanner {
                    attentionBanner
                }

                popoverSearchField

                HStack(alignment: .top) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(L10n.App.Dashboard.title.localized)
                            .font(.headline.weight(.semibold))
                        Text(L10n.App.Redesign.Popover.systemHealth.localized)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    HealthBadgeView(status: core.aggregateHealth)
                }
                .padding(.top, 4)

                HStack(spacing: 8) {
                    MetricChipView(
                        label: L10n.App.Redesign.Popover.pendingUpdates.localized,
                        value: core.outdatedPackages.count
                    )
                    MetricChipView(
                        label: L10n.App.Redesign.Popover.failures.localized,
                        value: core.failedTaskCount
                    )
                    MetricChipView(
                        label: L10n.App.Redesign.Popover.runningTasks.localized,
                        value: core.runningTaskCount
                    )
                }

                managerSnapshotCard
                tasksCard
            }
            .padding(16)

            Divider()

            popoverFooter
                .padding(.horizontal, 14)
                .padding(.vertical, 10)
        }
        .frame(width: 400)
        .background(
            LinearGradient(
                colors: colorScheme == .dark
                    ? [
                        Color(nsColor: .windowBackgroundColor).opacity(0.9),
                        Color(nsColor: .underPageBackgroundColor).opacity(0.82)
                    ]
                    : [
                        Color.white.opacity(0.98),
                        Color(nsColor: .windowBackgroundColor).opacity(0.86)
                    ],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )
        )
    }

    private var attentionBanner: some View {
        HStack(spacing: 10) {
            Image(systemName: bannerSymbol)
                .foregroundStyle(bannerTint)
                .font(.system(size: 13, weight: .semibold))

            VStack(alignment: .leading, spacing: 2) {
                Text(bannerTitle)
                    .font(.caption.weight(.semibold))
                Text(bannerMessage)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }

            Spacer(minLength: 10)

            if core.outdatedPackages.count > 0 {
                Button(L10n.App.Settings.Action.upgradeAll.localized) {
                    context.showUpgradeSheet = true
                }
                .buttonStyle(UpdateAllPillButtonStyle())
                .helmPointer()
            } else {
                Button(L10n.Common.refresh.localized) {
                    core.triggerRefresh()
                }
                .buttonStyle(.plain)
                .font(.caption.weight(.semibold))
                .helmPointer()
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .background(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .fill(bannerTint.opacity(0.13))
                .overlay(
                    RoundedRectangle(cornerRadius: 12, style: .continuous)
                        .strokeBorder(bannerTint.opacity(0.18), lineWidth: 0.8)
                )
        )
    }

    private var popoverSearchField: some View {
        HStack(spacing: 8) {
            Image(systemName: "magnifyingglass")
                .foregroundStyle(.secondary)
            TextField(
                "app.redesign.popover.search_placeholder".localized,
                text: Binding(
                    get: { popoverSearchQuery },
                    set: { newValue in
                        popoverSearchQuery = newValue
                        syncSearchQuery(newValue)
                    }
                )
            )
            .textFieldStyle(.plain)
            .font(.subheadline)

            if core.isSearching {
                ProgressView()
                    .controlSize(.small)
            }

            if !popoverSearchQuery.isEmpty {
                Button {
                    popoverSearchQuery = ""
                    syncSearchQuery("")
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
                .helmPointer()
                .accessibilityLabel(L10n.Common.clear.localized)
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .background(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .fill(Color.primary.opacity(0.06))
                .overlay(
                    RoundedRectangle(cornerRadius: 10, style: .continuous)
                        .strokeBorder(Color.primary.opacity(0.08), lineWidth: 0.8)
                )
        )
        .onTapGesture {
            activeOverlay = .search
            isOverlaySearchFocused = true
        }
        .helmPointer()
        .accessibilityLabel(L10n.App.Redesign.Popover.searchPlaceholder.localized)
    }

    private var managerSnapshotCard: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text(L10n.App.Redesign.Popover.managerSnapshot.localized)
                    .font(.subheadline.weight(.semibold))
                Spacer()
                Button(L10n.App.Redesign.Action.openControlCenter.localized) {
                    context.selectedSection = .managers
                    onOpenControlCenter()
                }
                .buttonStyle(.plain)
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
                .helmPointer()
            }

            if managerRows.isEmpty {
                Text(L10n.App.Dashboard.State.emptyManagers.localized)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else {
                ForEach(managerRows.prefix(4)) { manager in
                    Button {
                        context.selectedManagerId = manager.id
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
                                .foregroundStyle(.secondary)
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
                Text(L10n.App.Redesign.Popover.activeTasks.localized)
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
                .foregroundStyle(.secondary)
                .helmPointer()
                .accessibilityLabel(L10n.App.Redesign.Action.openControlCenter.localized)
            }

            if popoverTasks.isEmpty {
                Text(L10n.App.Tasks.noRecentTasks.localized)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else {
                ForEach(popoverTasks) { task in
                    TaskRowView(task: task, onCancel: task.isRunning ? { core.cancelTask(task) } : nil)
                }
            }
        }
        .padding(12)
        .background(cardBackground)
    }

    private var popoverFooter: some View {
        HStack(spacing: 10) {
            Button(L10n.App.Redesign.Popover.version.localized(with: ["version": helmVersion])) {
                activeOverlay = .about
            }
            .buttonStyle(.plain)
            .font(.caption2)
            .foregroundStyle(.secondary)
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
                title: L10n.App.Redesign.Overlay.Search.title.localized,
                onClose: closeOverlay
            ) {
                searchOverlayContent
            }
        case .quickSettings:
            PopoverOverlayCard(
                title: L10n.App.Redesign.Overlay.Settings.title.localized,
                onClose: closeOverlay
            ) {
                settingsOverlayContent
            }
        case .about:
            PopoverOverlayCard(
                title: L10n.App.Redesign.Overlay.About.title.localized,
                onClose: closeOverlay
            ) {
                aboutOverlayContent
            }
        case .confirmQuit:
            PopoverOverlayCard(
                title: L10n.App.Redesign.Overlay.Quit.title.localized,
                onClose: closeOverlay
            ) {
                quitOverlayContent
            }
        }
    }

    private var searchOverlayContent: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 8) {
                Image(systemName: "magnifyingglass")
                    .foregroundStyle(.secondary)
                TextField(
                    L10n.App.Redesign.Popover.searchPlaceholder.localized,
                    text: Binding(
                        get: { popoverSearchQuery },
                        set: { newValue in
                            popoverSearchQuery = newValue
                            syncSearchQuery(newValue)
                        }
                    )
                )
                .textFieldStyle(.plain)
                .font(.subheadline)
                .focused($isOverlaySearchFocused)

                if core.isSearching {
                    ProgressView()
                        .controlSize(.small)
                }
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            .background(
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .fill(Color.primary.opacity(0.06))
            )

            if searchResults.isEmpty && !popoverSearchQuery.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                Text(L10n.App.Redesign.Overlay.Search.empty.localized)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.vertical, 8)
            } else {
                ScrollView {
                    VStack(spacing: 6) {
                        ForEach(searchResults) { result in
                            Button {
                                context.selectedPackageId = result.id
                                context.selectedManagerId = result.managerId
                                context.selectedSection = .packages
                                onOpenControlCenter()
                                closeOverlay()
                            } label: {
                                HStack(spacing: 8) {
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(result.name)
                                            .font(.subheadline.weight(.medium))
                                            .lineLimit(1)
                                        Text(result.manager)
                                            .font(.caption2)
                                            .foregroundStyle(.secondary)
                                    }
                                    Spacer()
                                    if let latest = result.latestVersion {
                                        Text(latest)
                                            .font(.caption.monospacedDigit())
                                            .foregroundStyle(Color.orange)
                                    } else {
                                        Text(result.version)
                                            .font(.caption.monospacedDigit())
                                            .foregroundStyle(.secondary)
                                    }
                                }
                                .padding(.horizontal, 10)
                                .padding(.vertical, 8)
                                .background(
                                    RoundedRectangle(cornerRadius: 8, style: .continuous)
                                        .fill(Color.primary.opacity(0.05))
                                )
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                            .helmPointer()
                            .accessibilityElement(children: .combine)
                        }
                    }
                }
                .frame(maxHeight: 310)
            }

            HStack(spacing: 8) {
                Button(L10n.Common.cancel.localized) {
                    popoverSearchQuery = ""
                    syncSearchQuery("")
                    closeOverlay()
                }
                .buttonStyle(HelmSecondaryButtonStyle())
                .helmPointer()

                Spacer()

                Button(L10n.App.Redesign.Overlay.Search.openPackages.localized) {
                    context.selectedSection = .packages
                    onOpenControlCenter()
                    closeOverlay()
                }
                .buttonStyle(HelmPrimaryButtonStyle())
                .disabled(popoverSearchQuery.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                .helmPointer(enabled: !popoverSearchQuery.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
    }

    private var settingsOverlayContent: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack {
                Text(L10n.App.Settings.Label.language.localized)
                Spacer()
                Picker("", selection: $localization.currentLocale) {
                    Text(L10n.App.Settings.Label.systemDefaultWithEnglish.localized).tag("en")
                    Text(L10n.App.Settings.Label.spanish.localized).tag("es")
                    Text(L10n.App.Settings.Label.german.localized).tag("de")
                    Text(L10n.App.Settings.Label.french.localized).tag("fr")
                    Text(L10n.App.Settings.Label.portugueseBrazilian.localized).tag("pt-BR")
                    Text(L10n.App.Settings.Label.japanese.localized).tag("ja")
                }
                .labelsHidden()
                .frame(width: 180)
            }

            Toggle(
                L10n.App.Settings.Label.safeMode.localized,
                isOn: Binding(
                    get: { core.safeModeEnabled },
                    set: { core.setSafeMode($0) }
                )
            )
            .toggleStyle(.switch)

            Toggle(
                L10n.App.Settings.Label.autoCleanKegs.localized,
                isOn: Binding(
                    get: { core.homebrewKegAutoCleanupEnabled },
                    set: { core.setHomebrewKegAutoCleanup($0) }
                )
            )
            .toggleStyle(.switch)

            HStack(spacing: 8) {
                Button(L10n.App.Settings.Action.refreshNow.localized) {
                    core.triggerRefresh()
                    closeOverlay()
                }
                .buttonStyle(HelmSecondaryButtonStyle())
                .disabled(core.isRefreshing)
                .helmPointer(enabled: !core.isRefreshing)

                Spacer()

                Button(L10n.App.Redesign.Overlay.Settings.openAdvanced.localized) {
                    context.selectedSection = .settings
                    onOpenControlCenter()
                    closeOverlay()
                }
                .buttonStyle(HelmPrimaryButtonStyle())
                .helmPointer()
            }
        }
    }

    private var aboutOverlayContent: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 10) {
                Image("MenuBarIcon")
                    .resizable()
                    .renderingMode(.template)
                    .foregroundStyle(.primary)
                    .scaledToFit()
                    .frame(width: 22, height: 22)
                VStack(alignment: .leading, spacing: 2) {
                    Text(L10n.App.Redesign.Overlay.About.name.localized)
                        .font(.headline)
                    Text(L10n.App.Redesign.Overlay.About.subtitle.localized)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
            }

            Text(L10n.App.Redesign.Overlay.About.version.localized(with: ["version": helmVersion]))
                .font(.caption)

            Text(L10n.App.Redesign.Overlay.About.summary.localized(with: [
                "managers": core.visibleManagers.count,
                "updates": core.outdatedPackages.count
            ]))
            .font(.caption)
            .foregroundStyle(.secondary)

            HStack {
                Spacer()
                Button(L10n.Common.ok.localized) {
                    closeOverlay()
                }
                .buttonStyle(HelmPrimaryButtonStyle())
                .helmPointer()
            }
        }
    }

    private var quitOverlayContent: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(L10n.App.Redesign.Overlay.Quit.message.localized(with: ["tasks": core.runningTaskCount]))
                .font(.callout)
                .foregroundStyle(.secondary)

            HStack {
                Button(L10n.Common.cancel.localized) {
                    closeOverlay()
                }
                .buttonStyle(HelmSecondaryButtonStyle())
                .helmPointer()
                Spacer()
                Button(L10n.App.Settings.Action.quit.localized, role: .destructive) {
                    NSApplication.shared.terminate(nil)
                }
                .buttonStyle(.borderedProminent)
                .helmPointer()
            }
        }
    }

    private var bannerSymbol: String {
        if !core.isConnected {
            return "bolt.horizontal.circle.fill"
        }
        if core.failedTaskCount > 0 {
            return "exclamationmark.octagon.fill"
        }
        return "arrow.up.circle.fill"
    }

    private var bannerTint: Color {
        if !core.isConnected || core.failedTaskCount > 0 {
            return .red
        }
        return .orange
    }

    private var bannerTitle: String {
        if !core.isConnected {
            return L10n.App.Redesign.Popover.Banner.disconnectedTitle.localized
        }
        if core.failedTaskCount > 0 {
            return L10n.App.Redesign.Popover.Banner.failedTitle.localized(with: ["count": core.failedTaskCount])
        }
        return L10n.App.Redesign.Popover.Banner.updatesTitle.localized(with: ["count": core.outdatedPackages.count])
    }

    private var bannerMessage: String {
        if !core.isConnected {
            return L10n.App.Redesign.Popover.Banner.disconnectedMessage.localized
        }
        if core.failedTaskCount > 0 {
            return L10n.App.Redesign.Popover.Banner.failedMessage.localized
        }
        return L10n.App.Redesign.Popover.Banner.updatesMessage.localized
    }

    private var cardBackground: some View {
        RoundedRectangle(cornerRadius: 12, style: .continuous)
            .fill(colorScheme == .dark ? AnyShapeStyle(.ultraThinMaterial) : AnyShapeStyle(Color.white.opacity(0.9)))
            .overlay(
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .strokeBorder(Color.primary.opacity(0.08), lineWidth: 0.8)
            )
    }

    private func closeOverlay() {
        activeOverlay = nil
        context.popoverOverlayRequest = nil
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
                        .fill(Color.primary.opacity(0.08))
                )
        }
        .buttonStyle(.plain)
        .helmPointer()
        .accessibilityLabel(accessibilityText ?? symbol)
    }
}

private struct PopoverOverlayCard<Content: View>: View {
    @Environment(\.colorScheme) private var colorScheme
    let title: String
    let onClose: () -> Void
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text(title)
                    .font(.headline)
                Spacer()
                Button(action: onClose) {
                    Image(systemName: "xmark.circle.fill")
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
                .keyboardShortcut(.escape, modifiers: [])
                .helmPointer()
                .accessibilityLabel(L10n.Common.close.localized)
            }

            content
        }
        .padding(16)
        .frame(width: 360)
        .background(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .fill(
                    colorScheme == .dark
                        ? AnyShapeStyle(.ultraThinMaterial)
                        : AnyShapeStyle(Color.white.opacity(0.97))
                )
                .overlay(
                    RoundedRectangle(cornerRadius: 14, style: .continuous)
                        .strokeBorder(Color.primary.opacity(0.08), lineWidth: 0.8)
                )
                .shadow(color: Color.black.opacity(colorScheme == .dark ? 0.2 : 0.1), radius: 12, x: 0, y: 8)
        )
    }
}

private struct UpdateAllPillButtonStyle: ButtonStyle {
    @Environment(\.accessibilityReduceMotion) private var accessibilityReduceMotion

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.caption.weight(.bold))
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .foregroundStyle(Color.white)
            .background(
                Capsule(style: .continuous)
                    .fill(
                        LinearGradient(
                            colors: [Color.orange, Color.red.opacity(0.85)],
                            startPoint: .leading,
                            endPoint: .trailing
                        )
                    )
            )
            .scaleEffect(accessibilityReduceMotion ? 1 : (configuration.isPressed ? 0.97 : 1))
            .animation(
                accessibilityReduceMotion ? nil : .easeOut(duration: 0.1),
                value: configuration.isPressed
            )
    }
}

private struct MetricChipView: View {
    let label: String
    let value: Int

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(label)
                .font(.caption2)
                .foregroundStyle(.secondary)
            Text("\(value)")
                .font(.callout.monospacedDigit().weight(.semibold))
        }
        .padding(8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
        .accessibilityElement(children: .combine)
        .accessibilityLabel(label)
        .accessibilityValue("\(value)")
    }
}
