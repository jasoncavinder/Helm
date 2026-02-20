import SwiftUI
import AppKit

struct PopoverSearchOverlayContent: View {
    @ObservedObject private var core = HelmCore.shared
    @EnvironmentObject private var context: ControlCenterContext
    @Binding var popoverSearchQuery: String
    var isOverlaySearchFocused: FocusState<Bool>.Binding
    let searchResults: [PackageItem]
    let onSyncSearchQuery: (String) -> Void
    let onOpenControlCenter: () -> Void
    let onClose: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 8) {
                Image(systemName: "magnifyingglass")
                    .foregroundStyle(.secondary)
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
                .focused(isOverlaySearchFocused)

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
                                context.selectedTaskId = nil
                                context.selectedUpgradePlanStepId = nil
                                context.selectedSection = .packages
                                onOpenControlCenter()
                                onClose()
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
                                            .foregroundStyle(HelmTheme.stateAttention)
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
    @ObservedObject private var core = HelmCore.shared
    @ObservedObject private var appUpdate = AppUpdateCoordinator.shared
    let onClose: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 10) {
                Image("MenuBarIcon")
                    .resizable()
                    .renderingMode(.template)
                    .foregroundStyle(.primary)
                    .scaledToFit()
                    .frame(width: 22, height: 22)
                VStack(alignment: .leading, spacing: 2) {
                    Text(L10n.App.Overlay.About.name.localized)
                        .font(.headline)
                    Text(L10n.App.Overlay.About.subtitle.localized)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
            }

            Text(L10n.App.Overlay.About.version.localized(with: ["version": helmVersion]))
                .font(.caption)

            Text(L10n.App.Overlay.About.summary.localized(with: [
                "managers": core.visibleManagers.count,
                "updates": core.outdatedPackages.count
            ]))
            .font(.caption)
            .foregroundStyle(.secondary)

            HStack {
                if appUpdate.canCheckForUpdates {
                    Button(L10n.App.Overlay.About.checkForUpdates.localized) {
                        appUpdate.checkForUpdates()
                    }
                    .buttonStyle(HelmSecondaryButtonStyle())
                    .disabled(appUpdate.isCheckingForUpdates)
                    .helmPointer(enabled: !appUpdate.isCheckingForUpdates)
                } else if let unavailableKey = appUpdate.unavailableReasonLocalizationKey {
                    Text(unavailableKey.localized)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }

                Spacer()
                Button(L10n.Common.ok.localized) {
                    onClose()
                }
                .buttonStyle(HelmPrimaryButtonStyle())
                .helmPointer()
            }
        }
    }
}

struct PopoverQuitOverlayContent: View {
    @ObservedObject private var core = HelmCore.shared
    let onClose: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(L10n.App.Overlay.Quit.message.localized(with: ["tasks": core.runningTaskCount]))
                .font(.callout)
                .foregroundStyle(.secondary)

            HStack {
                Button(L10n.Common.cancel.localized) {
                    onClose()
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
}
