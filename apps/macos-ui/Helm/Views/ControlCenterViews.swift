import AppKit
import SwiftUI

struct ControlCenterWindowView: View {
    @EnvironmentObject private var context: ControlCenterContext
    @ObservedObject private var core = HelmCore.shared
    @ObservedObject private var walkthrough = WalkthroughManager.shared
    @Environment(\.colorScheme) private var colorScheme
    private let sidebarWidth: CGFloat = 232

    private var selectedSection: ControlCenterSection {
        context.selectedSection ?? .overview
    }

    private var searchQuery: Binding<String> {
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
        HStack(spacing: 0) {
            if context.isSidebarVisible {
                ControlCenterSidebarView(sidebarWidth: sidebarWidth)
                    .spotlightAnchor("ccSidebar")
                Divider()
            }

            if !selectedSection.supportsInspector || !context.isInspectorVisible {
                ControlCenterSectionHostView()
                    .frame(minWidth: 400, maxWidth: .infinity, maxHeight: .infinity)
            } else {
                HSplitView {
                    ControlCenterSectionHostView()
                        .frame(minWidth: 400, maxWidth: .infinity, maxHeight: .infinity)

                    ControlCenterInspectorView()
                        .frame(minWidth: 220, idealWidth: 280, maxWidth: 320)
                }
            }
        }
        .frame(minWidth: 860, minHeight: 600)
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
            ToolbarItem(placement: .navigation) {
                Button {
                    context.toggleSidebar()
                } label: {
                    Image(systemName: "sidebar.leading")
                }
                .help(
                    context.isSidebarVisible
                        ? "app.command.hide_sidebar".localized
                        : "app.command.show_sidebar".localized
                )
                .accessibilityLabel(
                    context.isSidebarVisible
                        ? "app.command.hide_sidebar".localized
                        : "app.command.show_sidebar".localized
                )
            }

            ToolbarItem(placement: .principal) {
                Text(selectedSection.title)
                    .font(.headline)
            }

            ToolbarItem(placement: .automatic) {
                ControlCenterToolbarSearchField(
                    text: searchQuery,
                    placeholder: L10n.App.ControlCenter.searchPlaceholder.localized,
                    focusToken: context.controlCenterSearchFocusToken
                )
                .frame(width: 260)
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

                Button(L10n.App.ControlCenter.upgradeAll.localized) {
                    context.presentUpgradeSheet(in: .controlCenter)
                    context.selectedSection = .updates
                }
                .buttonStyle(.borderedProminent)
                .disabled(core.outdatedPackages.isEmpty)
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
        .onChange(of: context.selectedSection) { newSection in
            context.alignInspectorSelection(for: newSection)
        }
        .onAppear {
            context.alignInspectorSelection(for: context.selectedSection)
            if core.hasCompletedOnboarding {
                core.triggerRefresh()
            }
            if !walkthrough.hasCompletedControlCenterWalkthrough {
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.8) {
                    walkthrough.startControlCenterWalkthrough()
                }
            }
        }
    }
}

private final class ControlCenterNativeSearchField: NSSearchField {
    private var focusRequestPending = false

    func requestFocus() {
        focusRequestPending = true
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
        }
    }
}

private struct ControlCenterToolbarSearchField: NSViewRepresentable {
    @Binding var text: String
    let placeholder: String
    let focusToken: Int

    func makeCoordinator() -> Coordinator {
        Coordinator(text: $text)
    }

    func makeNSView(context: Context) -> ControlCenterNativeSearchField {
        let searchField = ControlCenterNativeSearchField()
        searchField.delegate = context.coordinator
        searchField.placeholderString = placeholder
        searchField.setAccessibilityLabel(placeholder)
        return searchField
    }

    func updateNSView(_ searchField: ControlCenterNativeSearchField, context: Context) {
        context.coordinator.text = $text
        searchField.placeholderString = placeholder
        searchField.setAccessibilityLabel(placeholder)

        if searchField.stringValue != text {
            searchField.stringValue = text
        }

        guard context.coordinator.handledFocusToken != focusToken else { return }
        context.coordinator.handledFocusToken = focusToken
        searchField.requestFocus()
    }

    final class Coordinator: NSObject, NSSearchFieldDelegate {
        var text: Binding<String>
        var handledFocusToken = 0

        init(text: Binding<String>) {
            self.text = text
        }

        func controlTextDidChange(_ notification: Notification) {
            guard let searchField = notification.object as? NSSearchField else { return }
            text.wrappedValue = searchField.stringValue
        }
    }
}

private struct ControlCenterSidebarView: View {
    @EnvironmentObject private var context: ControlCenterContext
    @ObservedObject private var localization = LocalizationManager.shared
    @ObservedObject private var overviewState = HelmCore.shared.overviewState
    @Environment(\.colorScheme) private var colorScheme
    let sidebarWidth: CGFloat

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 11) {
                ZStack {
                    RoundedRectangle(cornerRadius: 10, style: .continuous)
                        .fill(
                            LinearGradient(
                                colors: [HelmTheme.blue700, HelmTheme.seaGlass],
                                startPoint: .topLeading,
                                endPoint: .bottomTrailing
                            )
                        )
                    Image(systemName: "helm")
                        .font(.system(size: 19, weight: .bold))
                        .foregroundColor(.white)
                }
                .frame(width: 40, height: 40)

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

            List(selection: $context.selectedSection) {
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
                    accessibilityLabel: ControlCenterSection.settings.title,
                    action: {
                        HelmApplicationWindowCommands.openSettings()
                    }
                ) {
                    Label(ControlCenterSection.settings.title, systemImage: ControlCenterSection.settings.icon)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }

                Spacer()

                HealthBadgeView(status: overviewState.aggregateHealth)
            }
            .font(.caption.weight(.medium))
            .padding(.horizontal, 18)
            .frame(height: 46)
        }
        .frame(width: sidebarWidth)
        .frame(maxHeight: .infinity, alignment: .top)
        .background(
            ControlCenterSidebarSurface(colorScheme: colorScheme)
        )
    }

    @ViewBuilder
    private func workspaceBadge(for section: ControlCenterSection) -> some View {
        if section == .updates, overviewState.outdatedPackagesCount > 0 {
            Text("\(overviewState.outdatedPackagesCount)")
                .font(.caption2.weight(.semibold).monospacedDigit())
                .padding(.horizontal, 6)
                .padding(.vertical, 2)
                .background(HelmTheme.surfaceElevated, in: Capsule())
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
            context.selectedSection = .managers
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

private struct ControlCenterSidebarSurface: View {
    let colorScheme: ColorScheme

    var body: some View {
        ZStack {
            Rectangle().fill(HelmTheme.surfacePanel)
            Rectangle()
                .fill(
                    LinearGradient(
                        colors: [
                            ControlCenterSidebarGradientPalette.topColor(for: colorScheme),
                            ControlCenterSidebarGradientPalette.bottomColor(for: colorScheme)
                        ],
                        startPoint: .top,
                        endPoint: .bottom
                    )
                )
        }
    }
}

private enum ControlCenterSidebarGradientPalette {
    static func topColor(for colorScheme: ColorScheme) -> Color {
        if colorScheme == .dark {
            return HelmTheme.blue900.opacity(0.22)
        }
        return HelmTheme.blue700.opacity(0.08)
    }

    static func bottomColor(for colorScheme: ColorScheme) -> Color {
        if colorScheme == .dark {
            return HelmTheme.surfaceBase.opacity(0.15)
        }
        return HelmTheme.surfacePanel.opacity(0.96)
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
        case .settings:
            SettingsSectionView()
                .spotlightAnchor("ccSettings")
        }
    }
}

// Section views and helper card types extracted to ControlCenterSectionViews.swift
