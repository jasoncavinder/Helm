import SwiftUI

extension SettingsPane {
    var title: String { titleKey.localized }
}

struct SettingsSectionView: View {
    @ObservedObject private var core = HelmCore.shared
    @ObservedObject private var appUpdate = AppUpdateCoordinator.shared
    @ObservedObject private var localization = LocalizationManager.shared
    @EnvironmentObject private var context: ControlCenterContext

    let selectedPane: SettingsPane
    let onResetCompleted: () -> Void

    @State private var showResetConfirmation = false
    @State private var isResetting = false
    @State private var includeDiagnostics = false
    @State private var showCopiedConfirmation = false
    @State private var showSupportOptionsModal = false
    @State private var supportTopGroupHeight: CGFloat = 0
    @State private var supportBottomButtonHeight: CGFloat = 0

    private let supportButtonSpacing: CGFloat = 8

    init(
        selectedPane: SettingsPane,
        onResetCompleted: @escaping () -> Void = {}
    ) {
        self.selectedPane = selectedPane
        self.onResetCompleted = onResetCompleted
    }

    private var helmCliStatusLabel: String {
        if !core.helmCliBundledAvailable {
            return L10n.App.Settings.CLI.Status.bundleUnavailable.localized
        }
        return core.helmCliShimInstalled
            ? L10n.App.Settings.CLI.Status.installed.localized
            : L10n.App.Settings.CLI.Status.notInstalled.localized
    }

    private var helmCliActionTitle: String {
        core.helmCliShimInstalled
            ? L10n.App.Settings.Action.removeCli.localized
            : L10n.App.Settings.Action.installCli.localized
    }

    private var cardFill: Color {
        HelmTheme.surfacePanel
    }

    private var supportButtonContentHeight: CGFloat? {
        guard supportTopGroupHeight > 0 else { return nil }
        // Match the adjacent two-button stack after accounting for label and style padding.
        return max(supportTopGroupHeight - 8, 0)
    }

    private var sendFeedbackButtonHeight: CGFloat? {
        guard supportBottomButtonHeight > 0 else { return nil }
        return supportBottomButtonHeight
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

    private func closeControlCenterWindowForOnboarding() {
        context.dismissUpgradeSheet()
        if let window = NSApp.windows.first(where: { $0 is ControlCenterWindow }) {
            window.performClose(nil)
        }
        onResetCompleted()
    }

    private func showsPane(_ pane: SettingsPane) -> Bool {
        selectedPane == pane
    }

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 14) {
                if showsPane(.general) {
                    SettingsCard(title: L10n.App.Settings.Section.general.localized, icon: "gearshape", fill: cardFill) {
                        HStack {
                            Text(L10n.App.Settings.Label.language.localized)
                            Spacer()
                            Picker("", selection: $localization.localePreference) {
                                Text(L10n.App.Settings.Label.systemDefault.localized)
                                    .tag(LocalizationPreferenceStore.systemSelection)
                                Text(L10n.App.Settings.Label.english.localized).tag("en")
                                Text(L10n.App.Settings.Label.spanish.localized).tag("es")
                                Text(L10n.App.Settings.Label.german.localized).tag("de")
                                Text(L10n.App.Settings.Label.french.localized).tag("fr")
                                Text(L10n.App.Settings.Label.portugueseBrazilian.localized).tag("pt-BR")
                                Text(L10n.App.Settings.Label.japanese.localized).tag("ja")
                                Text(L10n.App.Settings.Label.hungarian.localized).tag("hu")
                            }
                            .labelsHidden()
                            .frame(width: 220)
                        }

                        Divider()

                        Toggle(L10n.App.Settings.Label.launchAtLogin.localized, isOn: Binding(
                            get: { core.launchAtLoginEnabled },
                            set: { core.setLaunchAtLogin($0) }
                        ))
                        .toggleStyle(.switch)
                    }
                }

                if showsPane(.updates) {
                    SettingsCard(title: L10n.App.Section.updates.localized, icon: "arrow.triangle.2.circlepath", fill: cardFill) {
                        Toggle(L10n.App.Settings.Label.autoCheck.localized, isOn: Binding(
                            get: { appUpdate.autoCheckEnabled },
                            set: { appUpdate.setAutoCheckEnabled($0) }
                        ))
                        .toggleStyle(.switch)
                        .disabled(!appUpdate.canCheckForUpdates)

                        HStack {
                            Text(L10n.App.Settings.Label.checkFrequency.localized)
                            Spacer()
                            Picker("", selection: Binding(
                                get: { appUpdate.checkFrequencyMinutes },
                                set: { appUpdate.setCheckFrequencyMinutes($0) }
                            )) {
                                Text(L10n.App.Settings.Frequency.every1Hour.localized).tag(60)
                                Text(L10n.App.Settings.Frequency.daily.localized).tag(1_440)
                                Text(L10n.App.Settings.Frequency.weekly.localized).tag(10_080)
                                Text(L10n.App.Settings.Frequency.monthly.localized).tag(43_800)
                            }
                            .labelsHidden()
                            .frame(width: 150)
                            .disabled(!appUpdate.canCheckForUpdates || !appUpdate.autoCheckEnabled)
                        }

                        Divider()

                        VStack(alignment: .leading, spacing: 4) {
                            Toggle(L10n.App.Settings.Label.prereleaseUpdates.localized, isOn: Binding(
                                get: { appUpdate.prereleaseUpdatesEnabled },
                                set: { appUpdate.setPrereleaseUpdatesEnabled($0) }
                            ))
                            .toggleStyle(.switch)
                            .disabled(!appUpdate.canCheckForUpdates)

                            Text(L10n.App.Settings.Label.prereleaseUpdatesDescription.localized)
                                .font(.caption)
                                .foregroundColor(.secondary)
                        }

                        Divider()

                        VStack(alignment: .leading, spacing: 4) {
                            Toggle(L10n.App.Settings.Label.includeHelmInUpgradeAll.localized, isOn: Binding(
                                get: { appUpdate.includeHelmInUpgradeAll },
                                set: { appUpdate.setIncludeHelmInUpgradeAll($0) }
                            ))
                            .toggleStyle(.switch)
                            .disabled(!appUpdate.canCheckForUpdates)

                            Text(L10n.App.Settings.Label.includeHelmInUpgradeAllDescription.localized)
                                .font(.caption)
                                .foregroundColor(.secondary)
                        }
                    }
                }

                if showsPane(.cli) {
                    SettingsCard(title: L10n.App.Settings.CLI.section.localized, icon: "terminal", fill: cardFill) {
                        ServiceHealthStatusRow(
                            title: L10n.App.Settings.CLI.status.localized,
                            value: helmCliStatusLabel
                        )
                        ServiceHealthStatusRow(
                            title: L10n.App.Settings.CLI.shimPath.localized,
                            value: core.helmCliShimPath,
                            multiline: true
                        )
                        if let bundledPath = core.helmCliBundledPath, !bundledPath.isEmpty {
                            ServiceHealthStatusRow(
                                title: L10n.App.Settings.CLI.bundledPath.localized,
                                value: bundledPath,
                                multiline: true
                            )
                        }

                        Text(L10n.App.Settings.CLI.description.localized)
                            .font(.caption)
                            .foregroundColor(.secondary)

                        Divider()

                        SettingsActionButton(
                            title: helmCliActionTitle,
                            badges: [],
                            isProminent: false,
                            useSystemStyle: true
                        ) {
                            if core.helmCliShimInstalled {
                                core.removeHelmCliShim()
                            } else {
                                core.installHelmCliShim()
                            }
                        }
                        .disabled(
                            core.helmCliShimOperationInProgress ||
                            (!core.helmCliBundledAvailable && !core.helmCliShimInstalled)
                        )

                        if let statusMessage = core.helmCliShimStatusMessage, !statusMessage.isEmpty {
                            Text(statusMessage)
                                .font(.caption)
                                .foregroundColor(.secondary)
                        }
                    }
                }

                if showsPane(.sources) {
                    SettingsCard(title: L10n.App.Settings.Pane.sources.localized, icon: "point.3.connected.trianglepath.dotted", fill: cardFill) {
                        Toggle(L10n.App.Settings.Label.safeMode.localized, isOn: Binding(
                            get: { core.safeModeEnabled },
                            set: { core.setSafeMode($0) }
                        ))
                        .toggleStyle(.switch)

                        Divider()

                        Toggle(L10n.App.Settings.Label.autoCleanKegs.localized, isOn: Binding(
                            get: { core.homebrewKegAutoCleanupEnabled },
                            set: { core.setHomebrewKegAutoCleanup($0) }
                        ))
                        .toggleStyle(.switch)

                        Divider()

                        SettingsActionButton(
                            title: L10n.App.Settings.Action.restoreManagerPriority.localized,
                            badges: [],
                            isProminent: false,
                            useSystemStyle: true
                        ) {
                            core.restoreDefaultManagerPriorities()
                        }
                    }
                }

                if showsPane(.support) {
                    SettingsCard(title: L10n.App.Settings.SupportFeedback.section.localized, icon: "heart.fill", fill: cardFill) {
                        HStack(alignment: .top, spacing: supportButtonSpacing) {
                            VStack(spacing: supportButtonSpacing) {
                                SettingsActionButton(
                                    title: L10n.App.Settings.SupportFeedback.supportHelm.localized,
                                    badges: [],
                                    isProminent: true,
                                    leadingSymbol: "heart.circle.fill",
                                    alignLeading: false,
                                    minHeight: supportButtonContentHeight,
                                    contentVerticalPadding: 2,
                                    prominentStyleVerticalPadding: 2,
                                    titleFont: .callout.weight(.semibold)
                                ) {
                                    showSupportOptionsModal = true
                                }

                                SettingsActionButton(
                                    title: L10n.App.Settings.SupportFeedback.sendFeedback.localized,
                                    badges: [],
                                    isProminent: false,
                                    useSystemStyle: true
                                ) {
                                    HelmSupport.emailFeedback()
                                }
                                .frame(height: sendFeedbackButtonHeight)
                            }
                            .frame(maxWidth: .infinity, alignment: .top)

                            VStack(spacing: supportButtonSpacing) {
                                VStack(spacing: supportButtonSpacing) {
                                    SettingsActionButton(
                                        title: L10n.App.Settings.SupportFeedback.reportBug.localized,
                                        badges: [],
                                        isProminent: false,
                                        useSystemStyle: true
                                    ) {
                                        HelmSupport.reportBug(includeDiagnostics: includeDiagnostics)
                                        if includeDiagnostics {
                                            showCopiedBriefly()
                                        }
                                    }

                                    SettingsActionButton(
                                        title: L10n.App.Settings.SupportFeedback.requestFeature.localized,
                                        badges: [],
                                        isProminent: false,
                                        useSystemStyle: true
                                    ) {
                                        HelmSupport.requestFeature(includeDiagnostics: includeDiagnostics)
                                        if includeDiagnostics {
                                            showCopiedBriefly()
                                        }
                                    }
                                }
                                .background(
                                    GeometryReader { proxy in
                                        Color.clear.preference(
                                            key: SupportTopGroupHeightKey.self,
                                            value: proxy.size.height
                                        )
                                    }
                                )

                                SettingsActionButton(
                                    title: L10n.App.Settings.SupportFeedback.copyDiagnostics.localized,
                                    badges: [],
                                    isProminent: false,
                                    useSystemStyle: true
                                ) {
                                    HelmSupport.copyDiagnosticsToClipboard()
                                    showCopiedBriefly()
                                }
                                .background(
                                    GeometryReader { proxy in
                                        Color.clear.preference(
                                            key: SupportBottomButtonHeightKey.self,
                                            value: proxy.size.height
                                        )
                                    }
                                )

                                SettingsActionButton(
                                    title: L10n.App.Settings.SupportFeedback.copyStructuredExport.localized,
                                    badges: [],
                                    isProminent: false,
                                    useSystemStyle: true
                                ) {
                                    HelmSupport.copyStructuredDiagnosticsToClipboard()
                                    showCopiedBriefly()
                                }
                            }
                            .frame(maxWidth: .infinity, alignment: .top)
                        }
                        .onPreferenceChange(SupportTopGroupHeightKey.self) { height in
                            supportTopGroupHeight = height
                        }
                        .onPreferenceChange(SupportBottomButtonHeightKey.self) { height in
                            supportBottomButtonHeight = height
                        }

                        Divider()

                        Toggle(L10n.App.Settings.SupportFeedback.includeDiagnostics.localized, isOn: $includeDiagnostics)
                            .toggleStyle(.switch)
                            .font(.subheadline)

                        if showCopiedConfirmation {
                            HStack(spacing: 4) {
                                Image(systemName: "checkmark.circle.fill")
                                    .foregroundColor(.green)
                                Text(L10n.App.Settings.SupportFeedback.copiedConfirmation.localized)
                                    .foregroundColor(.secondary)
                            }
                            .font(.caption)
                            .transition(.opacity.combined(with: .scale))
                        }
                    }

                    SettingsCard(title: L10n.App.Settings.Section.advanced.localized, icon: "bolt.fill", fill: cardFill) {
                        SettingsActionButton(
                            title: L10n.App.Settings.Action.reset.localized,
                            badges: [],
                            isProminent: false,
                            useSystemStyle: true
                        ) {
                            showResetConfirmation = true
                        }
                        .disabled(core.isRefreshing || isResetting)
                    }
                }
            }
            .padding(20)
        }
        .alert(isPresented: $showResetConfirmation) {
            Alert(
                title: Text(L10n.App.Settings.Alert.Reset.title.localized),
                message: Text(L10n.App.Settings.Alert.Reset.message.localized),
                primaryButton: .default(Text(L10n.Common.reset.localized)) {
                    isResetting = true
                    core.resetDatabase { _ in
                        isResetting = false
                        closeControlCenterWindowForOnboarding()
                    }
                },
                secondaryButton: .cancel(Text(L10n.Common.cancel.localized))
            )
        }
        .sheet(isPresented: $showSupportOptionsModal) {
            SupportHelmOptionsModalView { channel in
                guard let url = channel.url else { return }
                HelmSupport.openURL(url)
                showSupportOptionsModal = false
            } onClose: {
                showSupportOptionsModal = false
            }
        }
        .onAppear {
            appUpdate.refreshState()
            core.refreshLaunchAtLogin()
            core.refreshHelmCliShimStatus()
        }
    }
}

struct SettingsWindowView: View {
    @Environment(\.dismiss) private var dismiss
    @ObservedObject private var localization = LocalizationManager.shared
    @State private var selectedPane: SettingsPane? = .general

    private var paneSelection: Binding<SettingsPane?> {
        Binding(
            get: { selectedPane },
            set: { newSelection in
                DispatchQueue.main.async {
                    guard selectedPane != newSelection else { return }
                    selectedPane = newSelection
                }
            }
        )
    }

    var body: some View {
        NavigationSplitView {
            List(SettingsPane.allCases, selection: paneSelection) { pane in
                Label(pane.title, systemImage: pane.icon)
                    .tag(pane)
            }
            .navigationTitle(L10n.App.Settings.Tab.title.localized)
            .navigationSplitViewColumnWidth(min: 150, ideal: 180, max: 220)
            .id(localization.currentLocale)
        } detail: {
            let pane = selectedPane ?? .general
            SettingsSectionView(
                selectedPane: pane,
                onResetCompleted: { dismiss() }
            )
            .navigationTitle(pane.title)
        }
        .frame(
            minWidth: 600,
            idealWidth: 680,
            maxWidth: 760,
            minHeight: 420,
            idealHeight: 500,
            maxHeight: 600
        )
    }
}

private struct SettingsCard<Content: View>: View {
    let title: String
    let icon: String
    let fill: Color
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Label(title, systemImage: icon)
                .font(.headline)

            content
                .font(.subheadline)
        }
        .padding(14)
        .background(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .fill(fill)
                .overlay(
                    RoundedRectangle(cornerRadius: 12, style: .continuous)
                        .strokeBorder(HelmTheme.borderSubtle.opacity(0.95), lineWidth: 0.8)
                )
        )
    }
}

private struct ServiceHealthStatusRow: View {
    let title: String
    let value: String
    let multiline: Bool
    let emphasize: Bool

    init(
        title: String,
        value: String,
        multiline: Bool = false,
        emphasize: Bool = false
    ) {
        self.title = title
        self.value = value
        self.multiline = multiline
        self.emphasize = emphasize
    }

    var body: some View {
        HStack(alignment: multiline ? .top : .firstTextBaseline, spacing: 10) {
            Text(title)
                .foregroundColor(HelmTheme.textSecondary)
            Spacer(minLength: 8)
            Text(value)
                .foregroundColor(emphasize ? .orange : HelmTheme.textPrimary)
                .font(multiline ? .caption : .subheadline.monospacedDigit())
                .multilineTextAlignment(.trailing)
                .lineLimit(multiline ? 3 : 1)
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel(title)
        .accessibilityValue(value)
    }
}

private struct SettingsActionButton: View {
    @Environment(\.isEnabled) private var isEnabled
    let title: String
    let badges: [SettingsActionBadge]
    let isProminent: Bool
    let useSystemStyle: Bool
    let leadingSymbol: String?
    let alignLeading: Bool
    let minHeight: CGFloat?
    let contentVerticalPadding: CGFloat
    let prominentStyleVerticalPadding: CGFloat?
    let overlayBadges: Bool
    let titleFont: Font
    let action: () -> Void

    init(
        title: String,
        badges: [SettingsActionBadge],
        isProminent: Bool,
        useSystemStyle: Bool = false,
        leadingSymbol: String? = nil,
        alignLeading: Bool = false,
        minHeight: CGFloat? = nil,
        contentVerticalPadding: CGFloat = 8,
        prominentStyleVerticalPadding: CGFloat? = nil,
        overlayBadges: Bool = false,
        titleFont: Font = .subheadline.weight(.semibold),
        action: @escaping () -> Void
    ) {
        self.title = title
        self.badges = badges
        self.isProminent = isProminent
        self.useSystemStyle = useSystemStyle
        self.leadingSymbol = leadingSymbol
        self.alignLeading = alignLeading
        self.minHeight = minHeight
        self.contentVerticalPadding = contentVerticalPadding
        self.prominentStyleVerticalPadding = prominentStyleVerticalPadding
        self.overlayBadges = overlayBadges
        self.titleFont = titleFont
        self.action = action
    }

    var body: some View {
        if useSystemStyle {
            Button(action: action) {
                labelContent
            }
            .buttonStyle(.bordered)
            .controlSize(.regular)
            .helmPointer(enabled: isEnabled)
        } else if isProminent {
            Button(action: action) {
                labelContent
            }
            .buttonStyle(
                HelmProButtonStyle(
                    verticalPadding: prominentStyleVerticalPadding ?? 8
                )
            )
            .controlSize(.regular)
            .helmPointer(enabled: isEnabled)
        } else {
            Button(action: action) {
                labelContent
            }
            .buttonStyle(HelmSecondaryButtonStyle())
            .controlSize(.regular)
            .helmPointer(enabled: isEnabled)
        }
    }

    private var labelContent: some View {
        Group {
            if overlayBadges && !badges.isEmpty {
                ZStack(alignment: .topLeading) {
                    titleRow
                        .frame(
                            maxWidth: .infinity,
                            maxHeight: .infinity,
                            alignment: alignLeading ? .leading : .center
                        )
                    badgeRow
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            } else {
                VStack(alignment: .leading, spacing: 6) {
                    if !badges.isEmpty {
                        badgeRow
                    }
                    titleRow
                }
            }
        }
        .frame(maxWidth: .infinity, minHeight: minHeight, alignment: .center)
        .padding(.vertical, contentVerticalPadding)
    }

    private var badgeRow: some View {
        HStack(spacing: 4) {
            ForEach(badges) { badge in
                SettingsBadgeView(badge: badge)
            }
            Spacer(minLength: 0)
        }
    }

    private var titleRow: some View {
        HStack(spacing: 6) {
            if alignLeading {
                if let leadingSymbol {
                    Image(systemName: leadingSymbol)
                        .font(.system(size: 12, weight: .semibold))
                }
                Text(title)
                    .font(titleFont)
                    .lineLimit(1)
                Spacer(minLength: 0)
            } else {
                Spacer(minLength: 0)
                if let leadingSymbol {
                    Image(systemName: leadingSymbol)
                        .font(.system(size: 12, weight: .semibold))
                }
                Text(title)
                    .font(titleFont)
                    .lineLimit(1)
                Spacer(minLength: 0)
            }
        }
    }
}

enum SupportHelmChannel: String, CaseIterable, Identifiable {
    case gitHubSponsors
    case patreon
    case buyMeACoffee
    case koFi
    case payPal
    case venmo

    var id: String { rawValue }

    var title: String {
        switch self {
        case .gitHubSponsors:
            return L10n.App.Settings.SupportFeedback.gitHubSponsors.localized
        case .patreon:
            return L10n.App.Settings.SupportFeedback.patreon.localized
        case .buyMeACoffee:
            return L10n.App.Settings.SupportFeedback.buyMeACoffee.localized
        case .koFi:
            return L10n.App.Settings.SupportFeedback.koFi.localized
        case .payPal:
            return L10n.App.Settings.SupportFeedback.payPal.localized
        case .venmo:
            return L10n.App.Settings.SupportFeedback.venmo.localized
        }
    }

    var symbol: String {
        switch self {
        case .gitHubSponsors:
            return "star.fill"
        case .patreon:
            return "heart.fill"
        case .buyMeACoffee:
            return "cup.and.saucer.fill"
        case .koFi:
            return "mug.fill"
        case .payPal:
            return "creditcard.fill"
        case .venmo:
            return "dollarsign.circle.fill"
        }
    }

    var url: URL? {
        switch self {
        case .gitHubSponsors:
            return HelmSupport.gitHubSponsorsURL
        case .patreon:
            return HelmSupport.patreonURL
        case .buyMeACoffee:
            return HelmSupport.buyMeACoffeeURL
        case .koFi:
            return HelmSupport.koFiURL
        case .payPal:
            return HelmSupport.payPalURL
        case .venmo:
            return HelmSupport.venmoURL
        }
    }
}

struct SupportHelmOptionsModalView: View {
    let onSelect: (SupportHelmChannel) -> Void
    let onClose: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack {
                Label(
                    L10n.App.Settings.SupportFeedback.supportHelm.localized,
                    systemImage: "heart.circle.fill"
                )
                .font(.title3.weight(.semibold))
                Spacer()
                Button(L10n.Common.cancel.localized, action: onClose)
                    .buttonStyle(.bordered)
            }

            LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 8) {
                ForEach(SupportHelmChannel.allCases) { channel in
                    Button {
                        onSelect(channel)
                    } label: {
                        HStack(spacing: 7) {
                            Image(systemName: channel.symbol)
                                .font(.system(size: 12, weight: .semibold))
                            Text(channel.title)
                                .lineLimit(1)
                            Spacer(minLength: 0)
                            if channel.url == nil {
                                Text(L10n.App.Managers.State.comingSoon.localized)
                                    .font(.caption2)
                                    .foregroundColor(HelmTheme.textSecondary)
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.vertical, 4)
                    }
                    .buttonStyle(HelmSecondaryButtonStyle())
                    .disabled(channel.url == nil)
                    .helmPointer(enabled: channel.url != nil)
                }
            }
        }
        .padding(18)
        .frame(width: 440)
    }
}

private struct SupportTopGroupHeightKey: PreferenceKey {
    static var defaultValue: CGFloat = 0

    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = max(value, nextValue())
    }
}

private struct SupportBottomButtonHeightKey: PreferenceKey {
    static var defaultValue: CGFloat = 0

    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = max(value, nextValue())
    }
}

private struct SettingsActionBadge: Identifiable {
    let id: String
    let managerId: String?
    let label: String
    let symbol: String?
    let tint: Color
}

private struct SettingsBadgeView: View {
    let badge: SettingsActionBadge

    var body: some View {
        HStack(spacing: 3) {
            if let symbol = badge.symbol {
                Image(systemName: symbol)
                    .font(.system(size: 8, weight: .bold))
            }
            Text(badge.label)
                .lineLimit(1)
        }
        .font(.caption2.weight(.semibold))
        .foregroundColor(badge.tint)
        .padding(.horizontal, 6)
        .padding(.vertical, 3)
        .background(
            Capsule(style: .continuous)
                .fill(badge.tint.opacity(0.15))
        )
        .overlay(
            Capsule(style: .continuous)
                .strokeBorder(badge.tint.opacity(0.2), lineWidth: 0.8)
        )
        .help(
            badge.managerId == "softwareupdate" && badge.symbol == "nosign"
                ? L10n.App.Settings.Label.safeMode.localized
                : badge.label
        )
        .accessibilityLabel(badge.label)
    }
}
