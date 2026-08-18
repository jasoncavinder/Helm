import AppKit
import SwiftUI

struct ControlCenterWindowView: View {
    @EnvironmentObject private var context: ControlCenterContext
    @ObservedObject private var core = HelmCore.shared
    @ObservedObject private var walkthrough = WalkthroughManager.shared
    @Environment(\.colorScheme) private var colorScheme
    private let sidebarWidth: CGFloat = 232

    private var firstRunMode: EnvironmentBriefFirstRunMode {
        EnvironmentBriefFirstRunConfiguration.mode()
    }

    private var presentsFirstRun: Bool {
        context.shouldPresentFirstRun(
            mode: firstRunMode,
            hasCompletedOnboarding: core.hasCompletedOnboarding
        )
    }

    private var selectedSection: ControlCenterSection {
        context.selectedSection ?? .overview
    }

    private var globalSearchQuery: Binding<String> {
        Binding(
            get: { context.searchQuery },
            set: { newValue in
                context.searchQuery = newValue
                core.searchText = newValue
                if !newValue.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                    context.selectedSection = .packages
                }
            }
        )
    }

    private var toolbarSearchQuery: Binding<String> {
        selectedSection == .updates
            ? Binding(
                get: { context.planPackageFilter },
                set: { context.planPackageFilter = $0 }
            )
            : globalSearchQuery
    }

    private var toolbarSearchPlaceholder: String {
        selectedSection == .updates
            ? L10n.App.Updates.filterSearchPlaceholder.localized
            : L10n.App.ControlCenter.searchPlaceholder.localized
    }

    private var planManagerScopeOptions: [String] {
        let researchManagers = WholeWorkflowResearchDatasetProvider.activePlanProjection()
            .map { Set($0.steps.map(\.managerID)) }
        let managers = researchManagers ?? Set(
            core.upgradePlanSteps
                .map(\.managerId)
                .filter(core.isManagerEnabled)
        )
        return [HelmCore.allManagersScopeId] + managers.sorted()
    }

    private func navigateToSection(for anchor: String) {
        switch anchor {
        case "ccOverview": context.selectedSection = .overview
        case "ccUpdates": context.selectedSection = .updates
        case "ccPackages": context.selectedSection = .packages
        case "ccTasks": context.selectedSection = .tasks
        case "ccManagers": context.selectedSection = .managers
        default: break
        }
    }

    private func deferInspectorAlignment(for section: ControlCenterSection?) {
        DispatchQueue.main.async {
            guard context.selectedSection == section else { return }
            context.alignInspectorSelection(for: section)
        }
    }

    var body: some View {
        Group {
            if presentsFirstRun {
                EnvironmentBriefFirstRunView(onComplete: completeFirstRun)
            } else {
                controlCenterContent
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(
            LinearGradient(
                colors: colorScheme == .dark
                    ? [
                        HelmTheme.surfaceBase,
                        HelmTheme.surfaceElevated.opacity(0.96)
                    ]
                    : [
                        HelmTheme.surfaceBase,
                        HelmTheme.surfacePanel
                    ],
                startPoint: .top,
                endPoint: .bottom
            )
        )
        .toolbar {
            if !presentsFirstRun {
                if selectedSection == .updates {
                    ToolbarItem(placement: .automatic) {
                        Picker(
                            L10n.App.Inspector.manager.localized,
                            selection: $context.planManagerScopeId
                        ) {
                            ForEach(planManagerScopeOptions, id: \.self) { managerID in
                                if managerID == HelmCore.allManagersScopeId {
                                    Text(L10n.App.Packages.Filter.allManagers.localized)
                                        .tag(managerID)
                                } else {
                                    Text(localizedManagerDisplayName(managerID))
                                        .tag(managerID)
                                }
                            }
                        }
                        .pickerStyle(.menu)
                        .frame(width: 170)
                    }
                }

                ToolbarItem(placement: .automatic) {
                    ControlCenterToolbarSearchField(
                        text: toolbarSearchQuery,
                        placeholder: toolbarSearchPlaceholder,
                        focusRouter: context.controlCenterSearchFocusRouter
                    )
                    .frame(width: selectedSection == .updates ? 250 : 320)
                }

                ToolbarItemGroup(placement: .primaryAction) {
                    if selectedSection.supportsInspector {
                        Button {
                            context.toggleInspector()
                        } label: {
                            Image(systemName: "sidebar.trailing")
                        }
                        .help(
                            context.isInspectorVisible
                                ? "app.command.hide_inspector".localized
                                : "app.command.show_inspector".localized
                        )
                        .accessibilityLabel(
                            context.isInspectorVisible
                                ? "app.command.hide_inspector".localized
                                : "app.command.show_inspector".localized
                        )
                    }

                    Button {
                        core.triggerRefresh()
                    } label: {
                        Label(L10n.Common.refresh.localized, systemImage: "arrow.clockwise")
                    }
                    .help(L10n.App.Settings.Action.refreshNow.localized)
                    .disabled(core.isRefreshing)

                    if !core.outdatedPackages.isEmpty {
                        Button {
                            context.presentUpgradeSheet(in: .controlCenter)
                            context.selectedSection = .updates
                        } label: {
                            Label(
                                L10n.App.ControlCenter.upgradeAll.localized,
                                systemImage: "arrow.up.circle.fill"
                            )
                        }
                        .buttonStyle(.borderedProminent)
                        .controlSize(.regular)
                        .fixedSize()
                        .disabled(!core.networkOperationsAvailable)
                    }
                }
            }
        }
        .sheet(
            isPresented: Binding(
                get: { context.showUpgradeSheet && context.upgradeSheetHost == .controlCenter },
                set: { isPresented in
                    if !isPresented {
                        context.dismissUpgradeSheet()
                    }
                }
            )
        ) {
            RedesignUpgradeSheetView()
                .environmentObject(context)
        }
        .onChange(of: walkthrough.currentStepIndex) { _ in
            guard walkthrough.isControlCenterWalkthroughActive,
                  let step = walkthrough.currentStep else { return }
            navigateToSection(for: step.targetAnchor)
        }
        .onChange(of: context.selectedSection) { newSection in
            deferInspectorAlignment(for: newSection)
        }
        .onAppear {
            deferInspectorAlignment(for: context.selectedSection)
            if core.hasCompletedOnboarding && !presentsFirstRun && !core.isRefreshing {
                core.triggerRefresh()
            }
            if !WholeWorkflowResearchDatasetProvider.isSelected(),
               !presentsFirstRun,
               !walkthrough.hasCompletedControlCenterWalkthrough {
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.8) {
                    walkthrough.startControlCenterWalkthrough()
                }
            }
        }
    }

    @ViewBuilder
    private var controlCenterContent: some View {
        ControlCenterHostedContentView(
            context: context,
            walkthrough: walkthrough,
            sidebarWidth: sidebarWidth
        ) {
            ControlCenterDetailView(
                context: context,
                walkthrough: walkthrough,
                isInspectorPresented: selectedSection.supportsInspector
                    && context.isInspectorVisible
            )
        }
    }

    private func completeFirstRun() {
        context.dismissFirstRunPreview()
        if !core.hasCompletedOnboarding {
            core.completeOnboarding()
            core.triggerRefresh()
        }
    }
}

private struct ControlCenterHostedContentView<Detail: View>: View {
    @ObservedObject var context: ControlCenterContext
    @ObservedObject var walkthrough: WalkthroughManager
    @State private var sidebarVisibility: NavigationSplitViewVisibility
    let sidebarWidth: CGFloat
    let detail: Detail

    init(
        context: ControlCenterContext,
        walkthrough: WalkthroughManager,
        sidebarWidth: CGFloat,
        @ViewBuilder detail: () -> Detail
    ) {
        self.context = context
        self.walkthrough = walkthrough
        self.sidebarWidth = sidebarWidth
        self.detail = detail()
        _sidebarVisibility = State(
            initialValue: NativeSidebarVisibilityPolicy.splitViewVisibility(
                isSidebarVisible: context.isSidebarVisible
            )
        )
    }

    var body: some View {
        NavigationSplitView(columnVisibility: $sidebarVisibility) {
            ControlCenterSidebarView()
                .navigationSplitViewColumnWidth(
                    min: 210,
                    ideal: sidebarWidth,
                    max: 280
                )
                .spotlightAnchor("ccSidebar")
        } detail: {
            detail
        }
        .environmentObject(context)
        .onChange(of: sidebarVisibility) { visibility in
            let isSidebarVisible = NativeSidebarVisibilityPolicy.isSidebarVisible(
                for: visibility
            )
            guard context.isSidebarVisible != isSidebarVisible else { return }
            DispatchQueue.main.async {
                guard context.isSidebarVisible != isSidebarVisible else { return }
                context.isSidebarVisible = isSidebarVisible
            }
        }
        .onChange(of: context.isSidebarVisible) { isSidebarVisible in
            let visibility = NativeSidebarVisibilityPolicy.splitViewVisibility(
                isSidebarVisible: isSidebarVisible
            )
            guard sidebarVisibility != visibility else { return }
            sidebarVisibility = visibility
        }
        .overlayPreferenceValue(SpotlightAnchorKey.self) { anchors in
            if walkthrough.isControlCenterWalkthroughActive {
                SpotlightOverlay(manager: walkthrough, anchors: anchors)
            }
        }
    }
}

private struct ControlCenterDetailView: View {
    @ObservedObject var context: ControlCenterContext
    @ObservedObject var walkthrough: WalkthroughManager
    let isInspectorPresented: Bool

    var body: some View {
        HSplitView {
            ControlCenterSectionHostView()
                .frame(minWidth: 320, maxWidth: .infinity, maxHeight: .infinity)
                .layoutPriority(1)

            if isInspectorPresented {
                ControlCenterHostedInspectorView(
                    context: context,
                    walkthrough: walkthrough
                )
                .frame(minWidth: 220, idealWidth: 280, maxWidth: 320)
                .layoutPriority(0)
            }
        }
    }
}

private struct ControlCenterHostedInspectorView: View {
    let context: ControlCenterContext
    @ObservedObject var walkthrough: WalkthroughManager

    var body: some View {
        ControlCenterInspectorView()
            .frame(
                minWidth: 220,
                idealWidth: 280,
                maxWidth: 320,
                maxHeight: .infinity
            )
            .environmentObject(context)
            .overlay {
                if walkthrough.isControlCenterWalkthroughActive {
                    Color.black.opacity(0.5)
                        .allowsHitTesting(true)
                }
            }
    }
}

private final class ControlCenterNativeSearchField: NSSearchField, ControlCenterSearchFocusTarget {
    private var focusRequestPending = false
    private var focusCompletion: (() -> Void)?

    func requestSearchFocus(completion: @escaping () -> Void) {
        focusRequestPending = true
        focusCompletion = completion
        fulfillFocusRequestIfPossible()
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        fulfillFocusRequestIfPossible()
    }

    private func fulfillFocusRequestIfPossible() {
        guard focusRequestPending, window != nil else { return }
        DispatchQueue.main.async { [weak self] in
            guard let self, self.window?.makeFirstResponder(self) == true else { return }
            self.focusRequestPending = false
            let completion = self.focusCompletion
            self.focusCompletion = nil
            completion?()
        }
    }
}

private struct ControlCenterToolbarSearchField: NSViewRepresentable {
    @Binding var text: String
    let placeholder: String
    let focusRouter: ControlCenterSearchFocusRouter

    func makeCoordinator() -> Coordinator {
        Coordinator(text: $text, focusRouter: focusRouter)
    }

    func makeNSView(context: Context) -> ControlCenterNativeSearchField {
        let searchField = ControlCenterNativeSearchField()
        searchField.delegate = context.coordinator
        searchField.placeholderString = placeholder
        searchField.setAccessibilityLabel(placeholder)
        focusRouter.attach(searchField)
        return searchField
    }

    func updateNSView(_ searchField: ControlCenterNativeSearchField, context: Context) {
        context.coordinator.text = $text
        searchField.placeholderString = placeholder
        searchField.setAccessibilityLabel(placeholder)

        let displayedText = context.coordinator.updateGate.displayedValue(modelValue: text)
        if searchField.stringValue != displayedText {
            context.coordinator.updateGate.applyModelValue {
                searchField.stringValue = displayedText
            }
        }
    }

    static func dismantleNSView(
        _ searchField: ControlCenterNativeSearchField,
        coordinator: Coordinator
    ) {
        searchField.delegate = nil
        coordinator.focusRouter?.detach(searchField)
    }

    final class Coordinator: NSObject, NSSearchFieldDelegate {
        var text: Binding<String>
        let updateGate = ControlCenterSearchTextUpdateGate()
        weak var focusRouter: ControlCenterSearchFocusRouter?

        init(text: Binding<String>, focusRouter: ControlCenterSearchFocusRouter) {
            self.text = text
            self.focusRouter = focusRouter
        }

        func controlTextDidChange(_ notification: Notification) {
            guard let searchField = notification.object as? NSSearchField else { return }
            guard updateGate.stageControlValue(
                searchField.stringValue,
                modelValue: text.wrappedValue
            ) else { return }
            DispatchQueue.main.async { [weak self] in
                guard let self, let value = updateGate.takePendingControlValue() else { return }
                guard text.wrappedValue != value else { return }
                text.wrappedValue = value
            }
        }
    }
}

private struct ControlCenterSidebarView: View {
    @EnvironmentObject private var context: ControlCenterContext
    @ObservedObject private var localization = LocalizationManager.shared
    @ObservedObject private var overviewState = HelmCore.shared.overviewState

    private var footerProjection: WayfinderProjectionContent {
        overviewState.wayfinderProjection.content
    }

    private var workspaceSelection: Binding<ControlCenterSection?> {
        Binding(
            get: { context.selectedSection },
            set: { newSelection in
                DispatchQueue.main.async {
                    guard context.selectedSection != newSelection else { return }
                    context.selectedSection = newSelection
                }
            }
        )
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 11) {
                Image(nsImage: NSApp.applicationIconImage)
                    .resizable()
                    .interpolation(.high)
                    .aspectRatio(contentMode: .fit)
                    .accessibilityHidden(true)
                    .frame(width: 50, height: 50)

                VStack(alignment: .leading, spacing: 1) {
                    Text(L10n.App.Dashboard.title.localized.uppercased())
                        .font(.system(.headline, design: .rounded, weight: .bold))
                        .tracking(1.5)
                    Text("app.wayfinder.sidebar.tagline".localized)
                        .font(.caption2.weight(.medium))
                        .foregroundColor(HelmTheme.textSecondary)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 16)
            .padding(.vertical, 16)

            List(selection: workspaceSelection) {
                Section {
                    ForEach(ControlCenterSection.wayfinderWorkspaces) { section in
                        HStack(spacing: 9) {
                            Label(section.title, systemImage: section.icon)
                            Spacer()
                            workspaceBadge(for: section)
                        }
                        .tag(section)
                        .accessibilityLabel(section.title)
                    }
                } header: {
                    Text("app.wayfinder.sidebar.workspace".localized)
                }
            }
            .listStyle(.sidebar)
            .scrollContentBackground(.hidden)

            Divider()

            environmentButton

            HStack(spacing: 8) {
                ControlCenterFooterRouteButton(
                    isSelected: false,
                    accessibilityLabel: L10n.App.Settings.Tab.title.localized,
                    action: {
                        context.settingsOpenRouter.requestOpen()
                    }
                ) {
                    Label(L10n.App.Settings.Tab.title.localized, systemImage: "gearshape")
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                .spotlightAnchor("ccSettings")

                Spacer()

                footerStatusView
            }
            .font(.caption.weight(.medium))
            .padding(.horizontal, 18)
            .frame(height: 46)
        }
        .frame(maxHeight: .infinity, alignment: .top)
        .background(WayfinderSidebarSurface())
    }

    @ViewBuilder
    private var footerStatusView: some View {
        if let status = footerProjection.condition.sidebarFooterStatus {
            if status.isActionable {
                Button {
                    context.navigate(to: footerProjection.primaryAction)
                } label: {
                    WayfinderFooterStatusBadge(status: status)
                }
                .buttonStyle(.plain)
                .helmPointer()
                .accessibilityHint(footerProjection.primaryActionTitle.localized)
            } else {
                WayfinderFooterStatusBadge(status: status)
            }
        }
    }

    @ViewBuilder
    private func workspaceBadge(for section: ControlCenterSection) -> some View {
        if section == .updates, overviewState.outdatedPackagesCount > 0 {
            Text("\(overviewState.outdatedPackagesCount)")
                .font(.caption2.weight(.semibold).monospacedDigit())
                .foregroundColor(
                    context.selectedSection == section
                        ? HelmTheme.blue700
                        : HelmTheme.textPrimary
                )
                .padding(.horizontal, 6)
                .padding(.vertical, 2)
                .background(
                    context.selectedSection == section
                        ? Color.white.opacity(0.95)
                        : HelmTheme.surfaceElevated,
                    in: Capsule()
                )
        } else if section == .tasks, overviewState.runningTaskCount > 0 {
            Circle()
                .fill(HelmTheme.seaGlass)
                .frame(width: 7, height: 7)
                .accessibilityLabel(L10n.App.Health.running.localized)
        }
    }

    private var environmentButton: some View {
        ControlCenterFooterRouteButton(
            isSelected: context.selectedSection == .managers,
            accessibilityLabel: ControlCenterSection.managers.title,
            accessibilityValue: "app.wayfinder.sidebar.sources_monitored".localized(
                with: ["count": "\(overviewState.visibleManagers.count)"]
            ),
            action: {
                context.select(.managers)
            }
        ) {
            HStack(spacing: 10) {
                Image(systemName: ControlCenterSection.managers.icon)
                    .frame(width: 18)
                VStack(alignment: .leading, spacing: 2) {
                    Text(ControlCenterSection.managers.title)
                        .font(.subheadline.weight(.semibold))
                    Text(
                        "app.wayfinder.sidebar.sources_monitored".localized(
                            with: ["count": "\(overviewState.visibleManagers.count)"]
                        )
                    )
                    .font(.caption2)
                    .foregroundColor(HelmTheme.textSecondary)
                }
                Spacer()
                Image(systemName: "chevron.right")
                    .font(.caption2.weight(.bold))
                    .foregroundColor(HelmTheme.textSecondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
    }
}

private struct ControlCenterSidebarButtonStyle: ButtonStyle {
    @Environment(\.accessibilityReduceMotion) private var accessibilityReduceMotion
    let isSelected: Bool
    let isHovered: Bool

    func makeBody(configuration: Configuration) -> some View {
        let backgroundOpacity: CGFloat = {
            if isSelected {
                return configuration.isPressed ? 0.24 : 0.16
            }
            if configuration.isPressed {
                return 0.1
            }
            if isHovered {
                return 0.06
            }
            return 0.001
        }()

        return configuration.label
            .foregroundColor(isSelected ? HelmTheme.blue500 : HelmTheme.textPrimary)
            .background(
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .fill(
                        isSelected
                            ? HelmTheme.blue500.opacity(backgroundOpacity)
                            : HelmTheme.textPrimary.opacity(backgroundOpacity)
                    )
                    .overlay(
                        RoundedRectangle(cornerRadius: 10, style: .continuous)
                            .strokeBorder(
                                isSelected
                                    ? HelmTheme.actionSecondaryBorder.opacity(0.45)
                                    : Color.clear,
                                lineWidth: 0.8
                            )
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

private struct ControlCenterFooterRouteButton<Label: View>: View {
    let isSelected: Bool
    let accessibilityLabel: String
    var accessibilityValue: String?
    let action: () -> Void
    @ViewBuilder let label: () -> Label

    @State private var isHovered = false

    var body: some View {
        let button = Button(action: action) {
            label()
                .font(.caption.weight(.medium))
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
        .accessibilityElement(children: .combine)
        .accessibilityLabel(accessibilityLabel)
        .accessibilityAddTraits(isSelected ? .isSelected : [])

        if let accessibilityValue,
           !accessibilityValue.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            button.accessibilityValue(accessibilityValue)
        } else {
            button
        }
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
        case .managers:
            ManagersSectionView()
                .spotlightAnchor("ccManagers")
        case .tasks:
            TasksSectionView()
                .spotlightAnchor("ccTasks")
        }
    }
}

// Section views and helper card types extracted to ControlCenterSectionViews.swift
