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

                if !context.searchQuery.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                    Button {
                        context.searchQuery = ""
                        core.searchText = ""
                    } label: {
                        Image(systemName: "xmark.circle.fill")
                            .foregroundStyle(.secondary)
                    }
                    .buttonStyle(.plain)
                    .helmPointer()
                    .accessibilityLabel(L10n.Common.clear.localized)
                }

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

// Section views and helper card types extracted to ControlCenterSectionViews.swift
