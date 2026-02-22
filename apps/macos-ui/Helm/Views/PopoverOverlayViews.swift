import SwiftUI
import AppKit

struct PopoverSearchOverlayContent: View {
    @ObservedObject private var core = HelmCore.shared
    @EnvironmentObject private var context: ControlCenterContext
    @Binding var popoverSearchQuery: String
    let searchResults: [ConsolidatedPackageItem]
    let onSyncSearchQuery: (String) -> Void
    let onOpenControlCenter: () -> Void
    let onClose: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 8) {
                Image(systemName: "magnifyingglass")
                    .foregroundColor(.secondary)
                TextField(
                    L10n.App.Popover.searchPlaceholder.localized,
                    text: Binding(
                        get: { popoverSearchQuery },
                        set: { newValue in
                            popoverSearchQuery = newValue
                            onSyncSearchQuery(newValue)
                        }
                    )
                )
                .textFieldStyle(.plain)
                .font(.subheadline)

                if core.isSearching {
                    ProgressView()
                        .controlSize(.small)
                }
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            .background(
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .fill(HelmTheme.surfacePanel)
                    .overlay(
                        RoundedRectangle(cornerRadius: 10, style: .continuous)
                            .strokeBorder(HelmTheme.borderSubtle.opacity(0.95), lineWidth: 0.8)
                    )
            )

            if searchResults.isEmpty && !popoverSearchQuery.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                Text(L10n.App.Overlay.Search.empty.localized)
                    .font(.caption)
                    .foregroundColor(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.vertical, 8)
            } else {
                ScrollView {
                    LazyVStack(spacing: 6) {
                        ForEach(searchResults) { result in
                            let package = result.package
                            HStack(spacing: 8) {
                                Button {
                                    context.selectedPackageId = package.id
                                    context.selectedManagerId = package.managerId
                                    context.selectedTaskId = nil
                                    context.selectedUpgradePlanStepId = nil
                                    context.selectedSection = .packages
                                    onOpenControlCenter()
                                    onClose()
                                } label: {
                                    HStack(spacing: 8) {
                                        VStack(alignment: .leading, spacing: 2) {
                                            Text(package.name)
                                                .font(.subheadline.weight(.medium))
                                                .lineLimit(1)
                                            Text(result.managerDisplayText)
                                                .font(.caption2)
                                                .foregroundColor(.secondary)
                                                .lineLimit(2)
                                        }
                                        Spacer()
                                        if let latest = package.latestVersion {
                                            Text(latest)
                                                .font(.caption.monospacedDigit())
                                                .foregroundColor(HelmTheme.stateAttention)
                                        } else {
                                            Text(package.version)
                                                .font(.caption.monospacedDigit())
                                                .foregroundColor(.secondary)
                                        }
                                    }
                                    .padding(.horizontal, 10)
                                    .padding(.vertical, 8)
                                    .frame(maxWidth: .infinity, alignment: .leading)
                                    .background(
                                        RoundedRectangle(cornerRadius: 8, style: .continuous)
                                            .fill(HelmTheme.surfaceElevated)
                                            .overlay(
                                                RoundedRectangle(cornerRadius: 8, style: .continuous)
                                                    .strokeBorder(HelmTheme.borderSubtle.opacity(0.9), lineWidth: 0.8)
                                            )
                                    )
                                    .contentShape(Rectangle())
                                }
                                .buttonStyle(.plain)
                                .helmPointer()
                                .accessibilityElement(children: .combine)

                                quickActionButtons(for: package)
                            }
                        }
                    }
                }
                .frame(maxHeight: 310)
            }

            HStack(spacing: 8) {
                Button(L10n.Common.cancel.localized) {
                    popoverSearchQuery = ""
                    onSyncSearchQuery("")
                    onClose()
                }
                .buttonStyle(HelmSecondaryButtonStyle())
                .helmPointer()

                Spacer()

                Button(L10n.App.Overlay.Search.openPackages.localized) {
                    context.selectedSection = .packages
                    onOpenControlCenter()
                    onClose()
                }
                .buttonStyle(HelmPrimaryButtonStyle())
                .disabled(popoverSearchQuery.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                .helmPointer(enabled: !popoverSearchQuery.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
    }

    @ViewBuilder
    private func quickActionButtons(for package: PackageItem) -> some View {
        HStack(spacing: 4) {
            if core.canInstallPackage(package) {
                iconActionButton(
                    symbol: "arrow.down.circle",
                    tooltip: L10n.App.Packages.Action.install.localized,
                    enabled: !core.installActionPackageIds.contains(package.id)
                ) {
                    core.installPackage(package)
                }
            }

            if core.canUninstallPackage(package) {
                iconActionButton(
                    symbol: "trash",
                    tooltip: L10n.App.Packages.Action.uninstall.localized,
                    enabled: !core.uninstallActionPackageIds.contains(package.id)
                ) {
                    core.uninstallPackage(package)
                }
            }

            if core.canUpgradeIndividually(package) {
                iconActionButton(
                    symbol: "arrow.up.circle",
                    tooltip: L10n.Common.update.localized,
                    enabled: !core.upgradeActionPackageIds.contains(package.id)
                ) {
                    core.upgradePackage(package)
                }
            }

            if core.canPinPackage(package) {
                let pinTooltip = package.pinned
                    ? L10n.App.Packages.Action.unpin.localized
                    : L10n.App.Packages.Action.pin.localized
                iconActionButton(
                    symbol: package.pinned ? "pin.slash" : "pin",
                    tooltip: pinTooltip,
                    enabled: !core.pinActionPackageIds.contains(package.id)
                ) {
                    if package.pinned {
                        core.unpinPackage(package)
                    } else {
                        core.pinPackage(package)
                    }
                }
            }
        }
    }

    private func iconActionButton(
        symbol: String,
        tooltip: String,
        enabled: Bool,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
        }
        .buttonStyle(HelmIconButtonStyle())
        .help(tooltip)
        .accessibilityLabel(tooltip)
        .disabled(!enabled)
        .helmPointer(enabled: enabled)
    }
}

struct PopoverSettingsOverlayContent: View {
    @ObservedObject private var core = HelmCore.shared
    @ObservedObject private var localization = LocalizationManager.shared
    @EnvironmentObject private var context: ControlCenterContext
    let onOpenControlCenter: () -> Void
    let onClose: () -> Void

    var body: some View {
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
                    Text(L10n.App.Settings.Label.hungarian.localized).tag("hu")
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
                    onClose()
                }
                .buttonStyle(HelmSecondaryButtonStyle())
                .disabled(core.isRefreshing)
                .helmPointer(enabled: !core.isRefreshing)

                Spacer()

                Button(L10n.App.Overlay.Settings.openAdvanced.localized) {
                    context.selectedSection = .settings
                    onOpenControlCenter()
                    onClose()
                }
                .buttonStyle(HelmPrimaryButtonStyle())
                .helmPointer()
            }
        }
    }
}

struct PopoverAboutOverlayContent: View {
    @ObservedObject private var overviewState = HelmCore.shared.overviewState
    @ObservedObject private var appUpdate = AppUpdateCoordinator.shared
    @State private var showSupportOptionsModal = false
    let onClose: () -> Void

    private var buildVersion: String {
        (Bundle.main.object(forInfoDictionaryKey: "CFBundleVersion") as? String)?
            .trimmingCharacters(in: .whitespacesAndNewlines) ?? "-"
    }

    private var channelLabel: String {
        switch appUpdate.distributionChannel {
        case .developerID:
            return L10n.App.Overlay.About.Channel.developerID.localized
        case .appStore:
            return L10n.App.Overlay.About.Channel.appStore.localized
        case .setapp:
            return L10n.App.Overlay.About.Channel.setapp.localized
        case .fleet:
            return L10n.App.Overlay.About.Channel.fleet.localized
        case .unknown:
            return L10n.App.Overlay.About.Channel.unknown.localized
        }
    }

    private var updateAuthorityLabel: String {
        switch appUpdate.updateAuthority {
        case .sparkle:
            return L10n.App.Overlay.About.UpdateAuthority.sparkle.localized
        case .appStore:
            return L10n.App.Overlay.About.UpdateAuthority.appStore.localized
        case .setapp:
            return L10n.App.Overlay.About.UpdateAuthority.setapp.localized
        case .adminControlled:
            return L10n.App.Overlay.About.UpdateAuthority.adminControlled.localized
        case .unavailable:
            return L10n.App.Overlay.About.UpdateAuthority.unavailable.localized
        }
    }

    private var lastCheckedLabel: String {
        guard let lastCheckDate = appUpdate.lastCheckDate else {
            return L10n.App.Overlay.About.never.localized
        }
        return Self.lastCheckFormatter.string(from: lastCheckDate)
    }

    private static let lastCheckFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.dateStyle = .medium
        formatter.timeStyle = .short
        return formatter
    }()

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 10) {
                Image("MenuBarIcon")
                    .resizable()
                    .renderingMode(.template)
                    .foregroundColor(.primary)
                    .scaledToFit()
                    .frame(width: 22, height: 22)
                VStack(alignment: .leading, spacing: 2) {
                    Text(L10n.App.Overlay.About.name.localized)
                        .font(.headline)
                    Text(L10n.App.Overlay.About.subtitle.localized)
                        .font(.caption)
                        .foregroundColor(.secondary)
                }
                Spacer()
            }

            VStack(alignment: .leading, spacing: 4) {
                Text(L10n.App.Overlay.About.version.localized(with: ["version": helmVersion]))
                    .font(.caption)
                Text(L10n.App.Overlay.About.build.localized(with: ["build": buildVersion]))
                    .font(.caption2)
                    .foregroundColor(.secondary)
            }

            Text(L10n.App.Overlay.About.summary.localized(with: [
                "managers": overviewState.visibleManagers.count,
                "updates": overviewState.outdatedPackagesCount
            ]))
            .font(.caption)
            .foregroundColor(.secondary)

            VStack(alignment: .leading, spacing: 4) {
                overlayDetailRow(
                    label: L10n.App.Overlay.About.channel.localized,
                    value: channelLabel
                )
                overlayDetailRow(
                    label: L10n.App.Overlay.About.updateAuthority.localized,
                    value: updateAuthorityLabel
                )
                overlayDetailRow(
                    label: L10n.App.Overlay.About.lastChecked.localized,
                    value: lastCheckedLabel
                )
            }
            .padding(.vertical, 2)

            if let unavailableKey = appUpdate.unavailableReasonLocalizationKey, !appUpdate.canCheckForUpdates {
                Text(unavailableKey.localized)
                    .font(.caption2)
                    .foregroundColor(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }

            if appUpdate.canCheckForUpdates {
                HStack {
                    Button(L10n.App.Overlay.About.checkForUpdates.localized) {
                        appUpdate.checkForUpdates()
                    }
                    .buttonStyle(HelmSecondaryButtonStyle())
                    .disabled(appUpdate.isCheckingForUpdates)
                    .helmPointer(enabled: !appUpdate.isCheckingForUpdates)

                    Spacer()
                }
            }

            HStack(spacing: 8) {

                Button(L10n.App.Legal.Action.viewTerms.localized) {
                    HelmSupport.openURL(HelmSupport.licenseTermsURL)
                }
                .buttonStyle(HelmSecondaryButtonStyle())
                .helmPointer()

                Button(L10n.App.Settings.SupportFeedback.supportHelm.localized) {
                    showSupportOptionsModal = true
                }
                .buttonStyle(HelmSecondaryButtonStyle())
                .helmPointer()

                Spacer()
                Button(L10n.Common.ok.localized) {
                    onClose()
                }
                .buttonStyle(HelmPrimaryButtonStyle())
                .helmPointer()
            }
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

    @ViewBuilder
    private func overlayDetailRow(label: String, value: String) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text(label)
                .font(.caption2)
                .foregroundColor(.secondary)
            Spacer()
            Text(value)
                .font(.caption2)
                .multilineTextAlignment(.trailing)
        }
    }
}

struct PopoverQuitOverlayContent: View {
    @ObservedObject private var overviewState = HelmCore.shared.overviewState
    let onClose: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(L10n.App.Overlay.Quit.message.localized(with: ["tasks": overviewState.runningTaskCount]))
                .font(.callout)
                .foregroundColor(.secondary)

            HStack {
                Button(L10n.Common.cancel.localized) {
                    onClose()
                }
                .buttonStyle(HelmSecondaryButtonStyle())
                .helmPointer()
                Spacer()
                Button(L10n.App.Settings.Action.quit.localized) {
                    NSApplication.shared.terminate(nil)
                }
                .buttonStyle(HelmPrimaryButtonStyle())
                .helmPointer()
            }
        }
    }
}
