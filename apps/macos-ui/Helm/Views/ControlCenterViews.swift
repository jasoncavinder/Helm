import SwiftUI
import AppKit

struct ControlCenterWindowView: View {
    @EnvironmentObject private var context: ControlCenterContext
    @ObservedObject private var core = HelmCore.shared
    @ObservedObject private var walkthrough = WalkthroughManager.shared
    @Environment(\.colorScheme) private var colorScheme
    private let sidebarWidth: CGFloat = 232

    private func navigateToSection(for anchor: String) {
        switch anchor {
        case "ccOverview": context.selectedSection = .overview
        case "ccUpdates": context.selectedSection = .updates
        case "ccPackages": context.selectedSection = .packages
        case "ccTasks": context.selectedSection = .tasks
        case "ccManagers": context.selectedSection = .managers
        case "ccSettings": context.selectedSection = .settings
        default: break
        }
    }

    var body: some View {
        VStack(spacing: 0) {
            ControlCenterTopBar(sidebarWidth: sidebarWidth)
            Divider()

            HStack(spacing: 0) {
                ControlCenterSidebarView(sidebarWidth: sidebarWidth)
                    .spotlightAnchor("ccSidebar")
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
        .overlayPreferenceValue(SpotlightAnchorKey.self) { anchors in
            if walkthrough.isControlCenterWalkthroughActive {
                SpotlightOverlay(manager: walkthrough, anchors: anchors)
            }
        }
        .onChange(of: walkthrough.currentStepIndex) { _ in
            guard walkthrough.isControlCenterWalkthroughActive,
                  let step = walkthrough.currentStep else { return }
            navigateToSection(for: step.targetAnchor)
        }
        .onAppear {
            if core.hasCompletedOnboarding {
                core.triggerRefresh()
            }
            if !walkthrough.hasCompletedControlCenterWalkthrough {
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.8) {
                    walkthrough.startControlCenterWalkthrough()
                }
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
            Text(L10n.App.Window.controlCenter.localized)
                .font(.headline.weight(.semibold))
                .padding(.leading, 72)

            Spacer(minLength: 20)

            HStack(spacing: 8) {
                Image(systemName: "magnifyingglass")
                    .foregroundStyle(.secondary)
                TextField(
                    L10n.App.ControlCenter.searchPlaceholder.localized,
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
            .accessibilityLabel(L10n.App.Settings.Action.refreshNow.localized)

            Button(L10n.App.ControlCenter.upgradeAll.localized) {
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
    @ObservedObject private var localization = LocalizationManager.shared
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
        .accessibilityLabel(section.title)
        .accessibilityAddTraits(isSelected ? .isSelected : [])
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
                .spotlightAnchor("ccOverview")
        case .updates:
            RedesignUpdatesSectionView()
                .spotlightAnchor("ccUpdates")
        case .packages:
            PackagesSectionView()
                .spotlightAnchor("ccPackages")
        case .tasks:
            TasksSectionView()
                .spotlightAnchor("ccTasks")
        case .managers:
            ManagersSectionView()
                .spotlightAnchor("ccManagers")
        case .settings:
            SettingsSectionView()
                .spotlightAnchor("ccSettings")
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
        .accessibilityElement(children: .combine)
        .accessibilityLabel(title)
        .accessibilityValue("\(value)")
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
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(title), \(authority.key.localized)")
        .accessibilityValue("\(status.key.localized), \(outdatedCount) \(L10n.App.Packages.Filter.upgradable.localized)")
    }
}
