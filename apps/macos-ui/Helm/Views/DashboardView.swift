import SwiftUI
import AppKit

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
            return "app.redesign.section.updates".localized
        case .packages:
            return L10n.App.Navigation.packages.localized
        case .tasks:
            return "app.redesign.section.tasks".localized
        case .managers:
            return L10n.App.Navigation.managers.localized
        case .settings:
            return L10n.App.Settings.Tab.title.localized
        }
    }

    var icon: String {
        switch self {
        case .overview:
            return "gauge.with.dots.needle.50percent"
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
            return "app.redesign.updates.authority.authoritative"
        case .standard:
            return "app.redesign.updates.authority.standard"
        case .guarded:
            return "app.redesign.updates.authority.guarded"
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
            return "app.redesign.health.healthy"
        case .attention:
            return "app.redesign.health.attention"
        case .error:
            return "app.redesign.health.error"
        case .running:
            return "app.redesign.health.running"
        case .notInstalled:
            return "app.redesign.health.not_installed"
        }
    }
}

final class ControlCenterContext: ObservableObject {
    @Published var selectedSection: ControlCenterSection? = .overview
    @Published var selectedManagerId: String?
    @Published var selectedPackageId: String?
    @Published var searchQuery: String = ""
    @Published var managerFilterId: String?
    @Published var showUpgradeSheet: Bool = false
    @Published var popoverOverlayRequest: PopoverOverlayRoute?
    @Published var popoverOverlayDismissToken: Int = 0
    @Published var popoverSearchFocusToken: Int = 0
    @Published var controlCenterSearchFocusToken: Int = 0
    @Published var isPopoverOverlayVisible: Bool = false
}

enum PopoverOverlayRoute: String, Identifiable {
    case search
    case quickSettings
    case about
    case confirmQuit

    var id: String { rawValue }
}

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
                        Text("app.redesign.popover.system_health".localized)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    HealthBadgeView(status: core.aggregateHealth)
                }
                .padding(.top, 4)

                HStack(spacing: 8) {
                    MetricChipView(
                        label: "app.redesign.popover.pending_updates".localized,
                        value: core.outdatedPackages.count
                    )
                    MetricChipView(
                        label: "app.redesign.popover.failures".localized,
                        value: core.failedTaskCount
                    )
                    MetricChipView(
                        label: "app.redesign.popover.running_tasks".localized,
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
    }

    private var managerSnapshotCard: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text("app.redesign.popover.manager_snapshot".localized)
                    .font(.subheadline.weight(.semibold))
                Spacer()
                Button("app.redesign.action.open_control_center".localized) {
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
                    .onTapGesture {
                        context.selectedManagerId = manager.id
                        context.selectedSection = .managers
                        onOpenControlCenter()
                    }
                    .helmPointer()
                }
            }
        }
        .padding(12)
        .background(cardBackground)
    }

    private var tasksCard: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text("app.redesign.popover.active_tasks".localized)
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
            }

            if popoverTasks.isEmpty {
                Text(L10n.App.Tasks.noRecentTasks.localized)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else {
                ForEach(popoverTasks) { task in
                    TaskRowView(task: task)
                }
            }
        }
        .padding(12)
        .background(cardBackground)
    }

    private var popoverFooter: some View {
        HStack(spacing: 10) {
            Button("app.redesign.popover.version".localized(with: ["version": helmVersion])) {
                activeOverlay = .about
            }
            .buttonStyle(.plain)
            .font(.caption2)
            .foregroundStyle(.secondary)
            .helmPointer()

            Spacer(minLength: 10)

            footerIconButton(symbol: "gearshape", action: {
                activeOverlay = .quickSettings
            })

            footerIconButton(symbol: "power", action: {
                activeOverlay = .confirmQuit
            })
        }
    }

    @ViewBuilder
    private func popoverOverlayView(for route: PopoverOverlayRoute) -> some View {
        switch route {
        case .search:
            PopoverOverlayCard(
                title: "app.redesign.overlay.search.title".localized,
                onClose: closeOverlay
            ) {
                searchOverlayContent
            }
        case .quickSettings:
            PopoverOverlayCard(
                title: "app.redesign.overlay.settings.title".localized,
                onClose: closeOverlay
            ) {
                settingsOverlayContent
            }
        case .about:
            PopoverOverlayCard(
                title: "app.redesign.overlay.about.title".localized,
                onClose: closeOverlay
            ) {
                aboutOverlayContent
            }
        case .confirmQuit:
            PopoverOverlayCard(
                title: "app.redesign.overlay.quit.title".localized,
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
                Text("app.redesign.overlay.search.empty".localized)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.vertical, 8)
            } else {
                ScrollView {
                    VStack(spacing: 6) {
                        ForEach(searchResults) { result in
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
                            .onTapGesture {
                                context.selectedPackageId = result.id
                                context.selectedManagerId = result.managerId
                                context.selectedSection = .packages
                                onOpenControlCenter()
                                closeOverlay()
                            }
                            .helmPointer()
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

                Button("app.redesign.overlay.search.open_packages".localized) {
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

                Button("app.redesign.overlay.settings.open_advanced".localized) {
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
                    Text("app.redesign.overlay.about.name".localized)
                        .font(.headline)
                    Text("app.redesign.overlay.about.subtitle".localized)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
            }

            Text("app.redesign.overlay.about.version".localized(with: ["version": helmVersion]))
                .font(.caption)

            Text("app.redesign.overlay.about.summary".localized(with: [
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
            Text("app.redesign.overlay.quit.message".localized(with: ["tasks": core.runningTaskCount]))
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
            return "app.redesign.popover.banner.disconnected.title".localized
        }
        if core.failedTaskCount > 0 {
            return "app.redesign.popover.banner.failed.title".localized(with: ["count": core.failedTaskCount])
        }
        return "app.redesign.popover.banner.updates.title".localized(with: ["count": core.outdatedPackages.count])
    }

    private var bannerMessage: String {
        if !core.isConnected {
            return "app.redesign.popover.banner.disconnected.message".localized
        }
        if core.failedTaskCount > 0 {
            return "app.redesign.popover.banner.failed.message".localized
        }
        return "app.redesign.popover.banner.updates.message".localized
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
    private func footerIconButton(symbol: String, action: @escaping () -> Void) -> some View {
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

struct HelmPrimaryButtonStyle: ButtonStyle {
    @Environment(\.accessibilityReduceMotion) private var accessibilityReduceMotion
    var cornerRadius: CGFloat = 10
    var horizontalPadding: CGFloat = 12
    var verticalPadding: CGFloat = 7

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.subheadline.weight(.semibold))
            .foregroundStyle(Color.white)
            .padding(.horizontal, horizontalPadding)
            .padding(.vertical, verticalPadding)
            .background(
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .fill(
                        LinearGradient(
                            colors: [Color.orange, Color.red.opacity(0.86)],
                            startPoint: .leading,
                            endPoint: .trailing
                        )
                    )
            )
            .scaleEffect(accessibilityReduceMotion ? 1 : (configuration.isPressed ? 0.98 : 1))
            .animation(
                accessibilityReduceMotion ? nil : .easeOut(duration: 0.12),
                value: configuration.isPressed
            )
    }
}

struct HelmSecondaryButtonStyle: ButtonStyle {
    @Environment(\.accessibilityReduceMotion) private var accessibilityReduceMotion
    var cornerRadius: CGFloat = 10
    var horizontalPadding: CGFloat = 12
    var verticalPadding: CGFloat = 7

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.subheadline.weight(.semibold))
            .foregroundStyle(Color.primary)
            .padding(.horizontal, horizontalPadding)
            .padding(.vertical, verticalPadding)
            .background(
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .fill(Color.primary.opacity(configuration.isPressed ? 0.14 : 0.09))
                    .overlay(
                        RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                            .strokeBorder(Color.primary.opacity(0.12), lineWidth: 0.8)
                    )
            )
            .scaleEffect(accessibilityReduceMotion ? 1 : (configuration.isPressed ? 0.985 : 1))
            .animation(
                accessibilityReduceMotion ? nil : .easeOut(duration: 0.12),
                value: configuration.isPressed
            )
    }
}

struct HelmIconButtonStyle: ButtonStyle {
    @Environment(\.accessibilityReduceMotion) private var accessibilityReduceMotion

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 12, weight: .semibold))
            .foregroundStyle(Color.primary)
            .frame(width: 28, height: 28)
            .background(
                Circle()
                    .fill(Color.primary.opacity(configuration.isPressed ? 0.14 : 0.09))
                    .overlay(
                        Circle()
                            .strokeBorder(Color.primary.opacity(0.12), lineWidth: 0.8)
                    )
            )
            .scaleEffect(accessibilityReduceMotion ? 1 : (configuration.isPressed ? 0.97 : 1))
            .animation(
                accessibilityReduceMotion ? nil : .easeOut(duration: 0.1),
                value: configuration.isPressed
            )
    }
}

struct ControlCenterWindowView: View {
    @EnvironmentObject private var context: ControlCenterContext
    @ObservedObject private var core = HelmCore.shared
    @Environment(\.colorScheme) private var colorScheme
    private let sidebarWidth: CGFloat = 232

    var body: some View {
        VStack(spacing: 0) {
            ControlCenterTopBar(sidebarWidth: sidebarWidth)
            Divider()

            HStack(spacing: 0) {
                ControlCenterSidebarView(sidebarWidth: sidebarWidth)
                Divider()
                HSplitView {
                    ControlCenterSectionHostView()
                        .frame(minWidth: 620, maxWidth: .infinity, maxHeight: .infinity)

                    ControlCenterInspectorView()
                        .frame(minWidth: 260, idealWidth: 280, maxWidth: 320)
                }
            }
        }
        .frame(minWidth: 1120, minHeight: 740)
        .background(
            LinearGradient(
                colors: colorScheme == .dark
                    ? [
                        Color(nsColor: .windowBackgroundColor),
                        Color(nsColor: .underPageBackgroundColor)
                    ]
                    : [
                        Color.white.opacity(0.98),
                        Color(nsColor: .windowBackgroundColor).opacity(0.88)
                    ],
                startPoint: .top,
                endPoint: .bottom
            )
        )
        .sheet(isPresented: $context.showUpgradeSheet) {
            RedesignUpgradeSheetView()
                .environmentObject(context)
        }
        .onAppear {
            if core.hasCompletedOnboarding {
                core.triggerRefresh()
            }
        }
        .ignoresSafeArea(.all, edges: .top)
    }
}

private struct ControlCenterTopBar: View {
    @EnvironmentObject private var context: ControlCenterContext
    @ObservedObject private var core = HelmCore.shared
    @Environment(\.colorScheme) private var colorScheme
    @FocusState private var isSearchFocused: Bool
    let sidebarWidth: CGFloat

    var body: some View {
        HStack(spacing: 12) {
            Text("app.redesign.window.control_center".localized)
                .font(.headline.weight(.semibold))
                .padding(.leading, 72)

            Spacer(minLength: 20)

            HStack(spacing: 8) {
                Image(systemName: "magnifyingglass")
                    .foregroundStyle(.secondary)
                TextField(
                    "app.redesign.control_center.search_placeholder".localized,
                    text: Binding(
                        get: { context.searchQuery },
                        set: { newValue in
                            context.searchQuery = newValue
                            core.searchText = newValue
                            if !newValue.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                                context.selectedSection = .packages
                            }
                        }
                    )
                )
                .textFieldStyle(.plain)
                .font(.subheadline)
                .focused($isSearchFocused)

                if core.isSearching {
                    ProgressView()
                        .controlSize(.small)
                }
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 7)
            .frame(width: 336)
            .background(
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .fill(Color.primary.opacity(0.06))
            )

            Button {
                core.triggerRefresh()
            } label: {
                Image(systemName: "arrow.clockwise")
            }
            .buttonStyle(HelmIconButtonStyle())
            .disabled(core.isRefreshing)
            .helmPointer(enabled: !core.isRefreshing)

            Button("app.redesign.control_center.upgrade_all".localized) {
                context.showUpgradeSheet = true
                context.selectedSection = .updates
            }
            .buttonStyle(HelmPrimaryButtonStyle())
            .disabled(core.outdatedPackages.isEmpty)
            .helmPointer(enabled: !core.outdatedPackages.isEmpty)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 4)
        .frame(height: 40)
        .background(
            HStack(spacing: 0) {
                ControlCenterSidebarSurface(colorScheme: colorScheme)
                    .frame(width: sidebarWidth)
                Rectangle().fill(.ultraThinMaterial)
            }
        )
        .onChange(of: context.controlCenterSearchFocusToken) { _ in
            isSearchFocused = true
        }
    }
}

private struct ControlCenterSidebarView: View {
    @EnvironmentObject private var context: ControlCenterContext
    @Environment(\.colorScheme) private var colorScheme
    let sidebarWidth: CGFloat

    var body: some View {
        ScrollView {
            VStack(spacing: 4) {
                ForEach(ControlCenterSection.allCases) { section in
                    ControlCenterSidebarRow(
                        section: section,
                        isSelected: (context.selectedSection ?? .overview) == section
                    ) {
                        context.selectedSection = section
                    }
                }
            }
            .padding(.horizontal, 8)
            .padding(.top, 10)
            .padding(.bottom, 12)
        }
        .frame(width: sidebarWidth)
        .frame(maxHeight: .infinity, alignment: .top)
        .background(
            ControlCenterSidebarSurface(colorScheme: colorScheme)
        )
    }
}

private struct ControlCenterSidebarSurface: View {
    let colorScheme: ColorScheme

    var body: some View {
        ZStack {
            Rectangle().fill(.regularMaterial)
            Rectangle()
                .fill(
                    LinearGradient(
                        colors: colorScheme == .dark
                            ? [
                                Color.white.opacity(0.08),
                                Color.black.opacity(0.28)
                            ]
                            : [
                                Color.black.opacity(0.12),
                                Color.black.opacity(0.05)
                            ],
                        startPoint: .top,
                        endPoint: .bottom
                    )
                )
        }
    }
}

private struct ControlCenterSidebarRow: View {
    let section: ControlCenterSection
    let isSelected: Bool
    let onSelect: () -> Void
    @State private var isHovered = false

    var body: some View {
        Button(action: onSelect) {
            HStack(spacing: 8) {
                Label(section.title, systemImage: section.icon)
                Spacer()
            }
            .font(.subheadline.weight(.medium))
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            .contentShape(Rectangle())
        }
        .buttonStyle(
            ControlCenterSidebarButtonStyle(
                isSelected: isSelected,
                isHovered: isHovered
            )
        )
        .onHover { isHovered = $0 }
        .helmPointer()
    }
}

private struct ControlCenterSidebarButtonStyle: ButtonStyle {
    @Environment(\.accessibilityReduceMotion) private var accessibilityReduceMotion
    let isSelected: Bool
    let isHovered: Bool

    func makeBody(configuration: Configuration) -> some View {
        let backgroundOpacity: CGFloat = {
            if isSelected {
                return configuration.isPressed ? 0.3 : 0.24
            }
            if configuration.isPressed {
                return 0.16
            }
            if isHovered {
                return 0.1
            }
            return 0.001
        }()

        return configuration.label
            .foregroundStyle(isSelected ? Color.accentColor : Color.primary)
            .background(
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .fill(
                        isSelected
                            ? Color.accentColor.opacity(backgroundOpacity)
                            : Color.primary.opacity(backgroundOpacity)
                    )
            )
            .scaleEffect(
                accessibilityReduceMotion
                    ? 1
                    : (configuration.isPressed ? 0.985 : 1)
            )
            .animation(
                accessibilityReduceMotion
                    ? nil
                    : .easeOut(duration: 0.12),
                value: configuration.isPressed
            )
    }
}

private struct ControlCenterSectionHostView: View {
    @EnvironmentObject private var context: ControlCenterContext

    var body: some View {
        switch context.selectedSection ?? .overview {
        case .overview:
            RedesignOverviewSectionView()
        case .updates:
            RedesignUpdatesSectionView()
        case .packages:
            PackagesSectionView()
        case .tasks:
            TasksSectionView()
        case .managers:
            ManagersSectionView()
        case .settings:
            SettingsSectionView()
        }
    }
}

private struct RedesignOverviewSectionView: View {
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
                        title: "app.redesign.popover.pending_updates".localized,
                        value: core.outdatedPackages.count
                    )
                    MetricCardView(
                        title: "app.redesign.popover.failures".localized,
                        value: core.failedTaskCount
                    )
                    MetricCardView(
                        title: "app.redesign.popover.running_tasks".localized,
                        value: core.runningTaskCount
                    )
                }

                Text("app.redesign.overview.manager_health".localized)
                    .font(.headline)

                LazyVGrid(columns: [GridItem(.adaptive(minimum: 220), spacing: 12)], spacing: 12) {
                    ForEach(core.visibleManagers) { manager in
                        ManagerHealthCardView(
                            title: localizedManagerDisplayName(manager.id),
                            authority: authority(for: manager.id),
                            status: core.health(forManagerId: manager.id),
                            outdatedCount: core.outdatedCount(forManagerId: manager.id)
                        )
                        .onTapGesture {
                            context.selectedManagerId = manager.id
                        }
                        .helmPointer()
                    }
                }

                Text("app.redesign.overview.recent_tasks".localized)
                    .font(.headline)

                if core.activeTasks.isEmpty {
                    Text(L10n.App.Tasks.noRecentTasks.localized)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                } else {
                    VStack(spacing: 0) {
                        ForEach(Array(core.activeTasks.prefix(10))) { task in
                            TaskRowView(task: task)
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

private struct RedesignUpdatesSectionView: View {
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
                    .filter { authority(forDisplayName: $0) == authorityLevel }
            )
            let count = previewBreakdown
                .filter { authority(forDisplayName: $0.manager) == authorityLevel }
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
                Button("app.redesign.action.refresh_plan".localized) {
                    core.triggerRefresh()
                }
                .buttonStyle(HelmSecondaryButtonStyle())
                .disabled(core.isRefreshing)
            }

            Text("app.redesign.updates.execution_plan".localized)
                .font(.headline)

            if !core.safeModeEnabled {
                Toggle("app.redesign.updates.include_os".localized, isOn: $includeOsUpdates)
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
                        Text("app.redesign.updates.managers".localized)
                            .foregroundStyle(.secondary)
                        Text("\(row.packageCount)")
                            .font(.body.monospacedDigit())
                        Text("app.redesign.updates.packages".localized)
                            .foregroundStyle(.secondary)
                    }
                    .padding(.vertical, 4)
                }
            }

            VStack(alignment: .leading, spacing: 6) {
                Text("app.redesign.updates.risk_flags".localized)
                    .font(.headline)
                riskRow(flag: "app.redesign.updates.risk.privileged".localized, active: requiresPrivileges)
                riskRow(flag: "app.redesign.updates.risk.reboot".localized, active: mayRequireReboot)
            }

            HStack {
                Button("app.redesign.action.dry_run".localized) {
                    let lines = previewBreakdown.prefix(8).map { "\($0.manager): \($0.count)" }
                    dryRunMessage = "app.redesign.dry_run.message".localized(with: [
                        "count": totalCount,
                        "summary": lines.joined(separator: "\n")
                    ])
                    showDryRun = true
                }
                .buttonStyle(HelmSecondaryButtonStyle())

                Button("app.redesign.action.run_plan".localized) {
                    core.upgradeAll(includePinned: false, allowOsUpdates: includeOsUpdates)
                }
                .buttonStyle(HelmPrimaryButtonStyle())
                .disabled(totalCount == 0)

                Spacer()
            }

            Spacer()
        }
        .padding(20)
        .alert("app.redesign.dry_run.title".localized, isPresented: $showDryRun) {
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

private struct ControlCenterInspectorView: View {
    @ObservedObject private var core = HelmCore.shared
    @EnvironmentObject private var context: ControlCenterContext

    private var selectedManager: ManagerInfo? {
        guard let managerId = context.selectedManagerId else { return nil }
        return ManagerInfo.all.first { $0.id == managerId }
    }

    private var selectedPackage: PackageItem? {
        guard let packageId = context.selectedPackageId else { return nil }
        return core.allKnownPackages.first { $0.id == packageId }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("app.redesign.inspector.title".localized)
                .font(.headline)

            if let package = selectedPackage {
                VStack(alignment: .leading, spacing: 8) {
                    Text(package.name)
                        .font(.title3.weight(.semibold))
                    Text("app.redesign.inspector.manager".localized)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Text(package.manager)
                        .font(.callout)
                    Text("app.redesign.inspector.installed".localized)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Text(package.version)
                        .font(.caption.monospaced())
                    if let latest = package.latestVersion {
                        Text("app.redesign.inspector.latest".localized)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        Text(latest)
                            .font(.caption.monospaced())
                    }
                    if let query = package.summary, !query.isEmpty {
                        Text("app.redesign.inspector.source_query".localized)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        Text(query)
                            .font(.caption)
                    }
                }
            } else if let manager = selectedManager {
                VStack(alignment: .leading, spacing: 8) {
                    Text(localizedManagerDisplayName(manager.id))
                        .font(.title3.weight(.semibold))
                    Text(authority(for: manager.id).key.localized)
                        .font(.callout)
                        .foregroundStyle(.secondary)
                    Text("app.redesign.inspector.capabilities".localized)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    ForEach(capabilities(for: manager), id: \.self) { capabilityKey in
                        Text(capabilityKey.localized)
                            .font(.caption)
                    }
                }
            } else {
                Text("app.redesign.inspector.empty".localized)
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }

            Spacer()
        }
        .padding(14)
    }
}

struct HealthBadgeView: View {
    let status: OperationalHealth

    var body: some View {
        Label(status.key.localized, systemImage: status.icon)
            .font(.caption.weight(.semibold))
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .foregroundStyle(status.color)
            .background(status.color.opacity(0.15), in: Capsule())
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
    }
}

private struct MetricCardView: View {
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
    }
}

private struct ManagerHealthCardView: View {
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
            Text("app.redesign.updates.execution_plan".localized)
                .font(.title3.weight(.semibold))

            if !core.safeModeEnabled {
                Toggle("app.redesign.updates.include_os".localized, isOn: $includeOsUpdates)
                    .toggleStyle(.switch)
            }

            HStack {
                Text("app.redesign.updates.authority.standard".localized)
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
                Button("app.redesign.action.dry_run".localized) {}
                    .buttonStyle(HelmSecondaryButtonStyle())
                    .disabled(true)
                Button("app.redesign.action.run_plan".localized) {
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

func localizedManagerDisplayName(_ managerId: String) -> String {
    switch managerId.lowercased() {
    case "homebrew_formula": return L10n.App.Managers.Name.homebrew.localized
    case "homebrew_cask": return L10n.App.Managers.Name.homebrewCask.localized
    case "npm", "npm_global": return L10n.App.Managers.Name.npm.localized
    case "pnpm": return L10n.App.Managers.Name.pnpm.localized
    case "yarn": return L10n.App.Managers.Name.yarn.localized
    case "poetry": return L10n.App.Managers.Name.poetry.localized
    case "rubygems": return L10n.App.Managers.Name.rubygems.localized
    case "bundler": return L10n.App.Managers.Name.bundler.localized
    case "pip": return L10n.App.Managers.Name.pip.localized
    case "pipx": return L10n.App.Managers.Name.pipx.localized
    case "cargo": return L10n.App.Managers.Name.cargo.localized
    case "cargo_binstall": return L10n.App.Managers.Name.cargoBinstall.localized
    case "mise": return L10n.App.Managers.Name.mise.localized
    case "rustup": return L10n.App.Managers.Name.rustup.localized
    case "softwareupdate": return L10n.App.Managers.Name.softwareUpdate.localized
    case "mas": return L10n.App.Managers.Name.appStore.localized
    default:
        return managerId.replacingOccurrences(of: "_", with: " ").capitalized
    }
}

func authority(for managerId: String) -> ManagerAuthority {
    switch managerId {
    case "mise", "rustup":
        return .authoritative
    case "homebrew_formula", "softwareupdate", "homebrew_cask":
        return .guarded
    default:
        return .standard
    }
}

private func authority(forDisplayName managerName: String) -> ManagerAuthority {
    if managerName == localizedManagerDisplayName("mise") || managerName == localizedManagerDisplayName("rustup") {
        return .authoritative
    }
    if managerName == localizedManagerDisplayName("homebrew_formula") || managerName == localizedManagerDisplayName("softwareupdate") {
        return .guarded
    }
    return .standard
}

private func capabilities(for manager: ManagerInfo) -> [String] {
    var result: [String] = [
        "app.redesign.capability.list",
        "app.redesign.capability.outdated",
    ]

    if manager.canInstall {
        result.append("app.redesign.capability.install")
    }

    if manager.canUninstall {
        result.append("app.redesign.capability.uninstall")
    }

    if manager.canUpdate {
        result.append("app.redesign.capability.upgrade")
    }

    if ["npm", "pnpm", "yarn", "pip", "cargo", "cargo_binstall", "poetry", "rubygems", "bundler"].contains(manager.id) {
        result.append("app.redesign.capability.search")
    }

    if manager.id == "homebrew_formula" {
        result.append("app.redesign.capability.pin")
    }

    return result
}

extension HelmCore {
    var allKnownPackages: [PackageItem] {
        let outdatedIds = Set(outdatedPackages.map(\.id))
        var combined = outdatedPackages
        combined.append(contentsOf: installedPackages.filter { !outdatedIds.contains($0.id) })

        let existing = Set(combined.map(\.id))
        combined.append(contentsOf: cachedAvailablePackages.filter { !existing.contains($0.id) })

        return combined
            .sorted { lhs, rhs in
                lhs.name.localizedCaseInsensitiveCompare(rhs.name) == .orderedAscending
            }
    }

    var visibleManagers: [ManagerInfo] {
        ManagerInfo.implemented.filter { manager in
            let status = managerStatuses[manager.id]
            let enabled = status?.enabled ?? true
            let detected = status?.detected ?? false
            return enabled && detected
        }
    }

    var failedTaskCount: Int {
        activeTasks.filter { $0.status.lowercased() == "failed" }.count
    }

    var runningTaskCount: Int {
        activeTasks.filter(\.isRunning).count
    }

    var aggregateHealth: OperationalHealth {
        if failedTaskCount > 0 {
            return .error
        }
        if runningTaskCount > 0 || isRefreshing {
            return .running
        }
        if !outdatedPackages.isEmpty {
            return .attention
        }
        return .healthy
    }

    func outdatedCount(forManagerId managerId: String) -> Int {
        outdatedPackages.filter { $0.managerId == managerId }.count
    }

    func health(forManagerId managerId: String) -> OperationalHealth {
        if let status = managerStatuses[managerId], status.detected == false {
            return .notInstalled
        }
        if managerStatuses[managerId] == nil && !detectedManagers.contains(managerId) {
            return .notInstalled
        }

        let managerName = localizedManagerDisplayName(managerId)

        let hasFailedTask = activeTasks.contains {
            $0.status.lowercased() == "failed" && $0.description.localizedCaseInsensitiveContains(managerName)
        }

        if hasFailedTask {
            return .error
        }
        if activeTasks.contains(where: {
            $0.isRunning && $0.description.localizedCaseInsensitiveContains(managerName)
        }) {
            return .running
        }
        if outdatedPackages.contains(where: { $0.managerId == managerId }) {
            return .attention
        }
        return .healthy
    }
}

private struct HelmPointerModifier: ViewModifier {
    let enabled: Bool

    func body(content: Content) -> some View {
        content
            .onHover { hovering in
                guard enabled else {
                    NSCursor.arrow.set()
                    return
                }
                if hovering {
                    NSCursor.pointingHand.set()
                } else {
                    NSCursor.arrow.set()
                }
            }
            .onDisappear {
                NSCursor.arrow.set()
            }
    }
}

extension View {
    func helmPointer(enabled: Bool = true) -> some View {
        modifier(HelmPointerModifier(enabled: enabled))
    }
}
