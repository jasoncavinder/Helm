import SwiftUI

struct SettingsSectionView: View {
    @ObservedObject private var core = HelmCore.shared
    @ObservedObject private var localization = LocalizationManager.shared
    @ObservedObject private var walkthrough = WalkthroughManager.shared

    @State private var checkFrequency = 60
    @State private var showResetConfirmation = false
    @State private var isResetting = false
    @State private var includeDiagnostics = false
    @State private var showCopiedConfirmation = false
    @State private var showSupportOptionsModal = false
    @State private var supportTopGroupHeight: CGFloat = 0
    @State private var supportBottomButtonHeight: CGFloat = 0

    private let supportButtonSpacing: CGFloat = 8

    private var selectedFrequencyLabel: String {
        switch checkFrequency {
        case 15:
            return L10n.App.Settings.Frequency.every15Min.localized
        case 30:
            return L10n.App.Settings.Frequency.every30Min.localized
        case 1440:
            return L10n.App.Settings.Frequency.daily.localized
        default:
            return L10n.App.Settings.Frequency.every1Hour.localized
        }
    }

    private var cardFill: Color {
        HelmTheme.surfacePanel
    }

    private var supportButtonHeight: CGFloat? {
        guard supportTopGroupHeight > 0 else { return nil }
        return supportTopGroupHeight
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

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                HStack {
                    Text(ControlCenterSection.settings.title)
                        .font(.title2.weight(.semibold))
                    Spacer()
                    HealthBadgeView(status: core.aggregateHealth)
                }

                HStack(spacing: 8) {
                    SettingsMetricPill(
                        title: L10n.App.Settings.Metric.managers.localized,
                        value: core.visibleManagers.count
                    )
                    SettingsMetricPill(
                        title: L10n.App.Settings.Metric.updates.localized,
                        value: core.outdatedPackages.count
                    )
                    SettingsMetricPill(
                        title: L10n.App.Settings.Metric.tasks.localized,
                        value: core.runningTaskCount
                    )
                }

                SettingsCard(title: L10n.App.Settings.Section.general.localized, icon: "globe", fill: cardFill) {
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
                        .frame(width: 220)
                    }

                    Divider()

                    HStack {
                        Text(L10n.App.Settings.Label.autoCheck.localized)
                        Spacer()
                        Text(L10n.App.Managers.State.comingSoon.localized)
                            .font(.caption)
                            .foregroundColor(.secondary)
                    }

                    HStack {
                        Text(L10n.App.Settings.Label.checkFrequency.localized)
                        Spacer()
                        Text(selectedFrequencyLabel)
                            .font(.caption)
                            .foregroundColor(.secondary)
                    }
                }

                SettingsCard(title: L10n.App.Settings.Section.managers.localized, icon: "shield.lefthalf.filled", fill: cardFill) {
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
                }

                SettingsCard(title: L10n.App.Settings.Section.advanced.localized, icon: "bolt.fill", fill: cardFill) {
                    LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 8) {
                        SettingsActionButton(
                            title: L10n.App.Settings.Action.refreshNow.localized,
                            badges: [],
                            isProminent: false,
                            useSystemStyle: true
                        ) {
                            core.triggerRefresh()
                        }
                        .disabled(core.isRefreshing)

                        SettingsActionButton(
                            title: L10n.App.Settings.Action.reset.localized,
                            badges: [],
                            isProminent: false,
                            useSystemStyle: true
                        ) {
                            showResetConfirmation = true
                        }
                        .disabled(core.isRefreshing || isResetting)

                        SettingsActionButton(
                            title: L10n.App.Settings.Action.quit.localized,
                            badges: [],
                            isProminent: false,
                            useSystemStyle: true
                        ) {
                            NSApplication.shared.terminate(nil)
                        }

                        SettingsActionButton(
                            title: L10n.App.Settings.Action.replayWalkthrough.localized,
                            badges: [],
                            isProminent: false,
                            useSystemStyle: true
                        ) {
                            walkthrough.resetWalkthroughs()
                            walkthrough.startPopoverWalkthrough()
                        }
                    }
                }

                SettingsCard(title: L10n.App.Settings.SupportFeedback.section.localized, icon: "heart.fill", fill: cardFill) {
                    HStack(alignment: .top, spacing: supportButtonSpacing) {
                        VStack(spacing: supportButtonSpacing) {
                            SettingsActionButton(
                                title: L10n.App.Settings.SupportFeedback.supportHelm.localized,
                                badges: [
                                    SettingsActionBadge(
                                        id: "support_helm_pro",
                                        managerId: nil,
                                        label: L10n.App.Settings.SupportFeedback.gitHubSponsors.localized,
                                        symbol: "star.fill",
                                        tint: HelmTheme.proAccent
                                    )
                                ],
                                isProminent: true,
                                leadingSymbol: "heart.circle.fill",
                                alignLeading: false,
                                contentVerticalPadding: 2,
                                prominentStyleVerticalPadding: 2,
                                overlayBadges: true,
                                titleFont: .callout.weight(.semibold)
                            ) {
                                showSupportOptionsModal = true
                            }
                            .frame(height: supportButtonHeight)

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
                                .foregroundStyle(.green)
                            Text(L10n.App.Settings.SupportFeedback.copiedConfirmation.localized)
                                .foregroundColor(.secondary)
                        }
                        .font(.caption)
                        .transition(.opacity.combined(with: .scale))
                    }
                }
            }
            .padding(20)
        }
        .alert(L10n.App.Settings.Alert.Reset.title.localized, isPresented: $showResetConfirmation) {
            Button(L10n.Common.cancel.localized, role: .cancel) {}
            Button(L10n.Common.reset.localized, role: .destructive) {
                isResetting = true
                core.resetDatabase { _ in
                    isResetting = false
                }
            }
        } message: {
            Text(L10n.App.Settings.Alert.Reset.message.localized)
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

private struct SettingsMetricPill: View {
    let title: String
    let value: Int

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title)
                .font(.caption2)
                .foregroundStyle(HelmTheme.textSecondary)
            Text("\(value)")
                .font(.callout.monospacedDigit().weight(.semibold))
                .foregroundStyle(HelmTheme.textPrimary)
        }
        .padding(8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: 8, style: .continuous)
                .fill(HelmTheme.surfaceElevated)
                .overlay(
                    RoundedRectangle(cornerRadius: 8, style: .continuous)
                        .strokeBorder(HelmTheme.borderSubtle.opacity(0.9), lineWidth: 0.8)
                )
        )
        .accessibilityElement(children: .combine)
        .accessibilityLabel(title)
        .accessibilityValue("\(value)")
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
                                    .foregroundStyle(HelmTheme.textSecondary)
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
        .foregroundStyle(badge.tint)
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

// Backward compatibility wrapper for legacy references.
struct SettingsPopoverView: View {
    var body: some View {
        SettingsSectionView()
            .frame(width: 440)
    }
}
