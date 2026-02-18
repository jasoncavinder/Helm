import SwiftUI

struct SettingsSectionView: View {
    @ObservedObject private var core = HelmCore.shared
    @ObservedObject private var localization = LocalizationManager.shared
    @EnvironmentObject private var context: ControlCenterContext
    @Environment(\.colorScheme) private var colorScheme

    @State private var checkFrequency = 60
    @State private var showResetConfirmation = false
    @State private var isResetting = false

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

    private var cardFill: AnyShapeStyle {
        if colorScheme == .dark {
            return AnyShapeStyle(.thinMaterial)
        }
        return AnyShapeStyle(Color.white.opacity(0.92))
    }

    private var refreshActionBadges: [SettingsActionBadge] {
        managerBadges(for: core.visibleManagers.map(\.id))
    }

    private var upgradeActionBadges: [SettingsActionBadge] {
        var managerIds = Set(core.outdatedPackages.map(\.managerId))
        if core.safeModeEnabled {
            managerIds.insert("softwareupdate")
        }

        return managerBadges(for: Array(managerIds)).map { badge in
            guard badge.managerId == "softwareupdate", core.safeModeEnabled else { return badge }
            return SettingsActionBadge(
                id: "softwareupdate-blocked",
                managerId: "softwareupdate",
                label: localizedManagerDisplayName("softwareupdate"),
                symbol: "nosign",
                tint: .red
            )
        }
    }

    private func managerBadges(for managerIds: [String], maxCount: Int = 3) -> [SettingsActionBadge] {
        let ordered = Array(Set(managerIds)).sorted {
            localizedManagerDisplayName($0).localizedCaseInsensitiveCompare(localizedManagerDisplayName($1)) == .orderedAscending
        }
        var badges = Array(ordered.prefix(maxCount)).map { managerId in
            SettingsActionBadge(
                id: managerId,
                managerId: managerId,
                label: localizedManagerDisplayName(managerId),
                symbol: managerSymbol(for: managerId),
                tint: authority(for: managerId) == .guarded ? .orange : .accentColor
            )
        }

        let hiddenCount = ordered.count - maxCount
        if hiddenCount > 0 {
            badges.append(
                SettingsActionBadge(
                    id: "more-\(hiddenCount)",
                    managerId: nil,
                    label: "+\(hiddenCount)",
                    symbol: nil,
                    tint: .secondary
                )
            )
        }
        return badges
    }

    private func managerSymbol(for managerId: String) -> String {
        switch managerId {
        case "softwareupdate":
            return "apple.logo"
        case "homebrew_formula", "homebrew_cask":
            return "cup.and.saucer.fill"
        case "mise", "rustup":
            return "wrench.and.screwdriver.fill"
        default:
            return "shippingbox.fill"
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
                        title: L10n.App.Redesign.Settings.Metric.managers.localized,
                        value: core.visibleManagers.count
                    )
                    SettingsMetricPill(
                        title: L10n.App.Redesign.Settings.Metric.updates.localized,
                        value: core.outdatedPackages.count
                    )
                    SettingsMetricPill(
                        title: L10n.App.Redesign.Settings.Metric.tasks.localized,
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
                            .foregroundStyle(.secondary)
                    }

                    HStack {
                        Text(L10n.App.Settings.Label.checkFrequency.localized)
                        Spacer()
                        Text(selectedFrequencyLabel)
                            .font(.caption)
                            .foregroundStyle(.secondary)
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
                            badges: refreshActionBadges,
                            isProminent: false
                        ) {
                            core.triggerRefresh()
                        }
                        .disabled(core.isRefreshing)

                        SettingsActionButton(
                            title: L10n.App.Settings.Action.upgradeAll.localized,
                            badges: upgradeActionBadges,
                            isProminent: true
                        ) {
                            context.showUpgradeSheet = true
                        }
                        .disabled(core.outdatedPackages.isEmpty)

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
    }
}

private struct SettingsCard<Content: View>: View {
    let title: String
    let icon: String
    let fill: AnyShapeStyle
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
                        .strokeBorder(Color.primary.opacity(0.08), lineWidth: 0.8)
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
                .foregroundStyle(.secondary)
            Text("\(value)")
                .font(.callout.monospacedDigit().weight(.semibold))
        }
        .padding(8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: 8, style: .continuous)
                .fill(Color.primary.opacity(0.06))
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
    let action: () -> Void

    init(
        title: String,
        badges: [SettingsActionBadge],
        isProminent: Bool,
        useSystemStyle: Bool = false,
        action: @escaping () -> Void
    ) {
        self.title = title
        self.badges = badges
        self.isProminent = isProminent
        self.useSystemStyle = useSystemStyle
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
            .buttonStyle(HelmPrimaryButtonStyle())
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
        VStack(alignment: .leading, spacing: 6) {
            if !badges.isEmpty {
                HStack(spacing: 4) {
                    ForEach(badges) { badge in
                        SettingsBadgeView(badge: badge)
                    }
                    Spacer(minLength: 0)
                }
            }
            HStack(spacing: 6) {
                Spacer(minLength: 0)
                Text(title)
                    .lineLimit(1)
                Spacer(minLength: 0)
            }
        }
        .font(.subheadline.weight(.semibold))
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.vertical, 8)
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
