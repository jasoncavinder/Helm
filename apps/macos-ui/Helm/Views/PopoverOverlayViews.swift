import SwiftUI
import AppKit

struct PopoverSearchOverlayContent: View {
    @ObservedObject private var core = HelmCore.shared
    @EnvironmentObject private var context: ControlCenterContext
    @State private var loadingPackageUninstallPreviewIds: Set<String> = []
    @State private var pendingPackageUninstall: PendingPackageUninstall?
    @Binding var popoverSearchQuery: String
    let searchResults: [ConsolidatedPackageItem]
    let onSyncSearchQuery: (String) -> Void
    let onOpenControlCenter: () -> Void
    let onClose: () -> Void

    private struct PendingPackageUninstall: Identifiable {
        let package: PackageItem
        let preview: PackageUninstallPreview?

        var id: String {
            if let preview {
                return "uninstall-\(package.id)-\(preview.blastRadiusScore)"
            }
            return "uninstall-\(package.id)-fallback"
        }
    }

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
                            let preferredManagerId = core.preferredManagerId(for: result.package)
                            let package = result.actionTarget(preferredManagerId: preferredManagerId)
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
                                            Text(package.displayName)
                                                .font(.subheadline.weight(.medium))
                                                .lineLimit(1)
                                            HStack(spacing: 4) {
                                                ForEach(Array(result.managerDisplayNames.enumerated()), id: \.offset) { _, managerName in
                                                    managerBadge(managerName)
                                                }
                                            }
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
        .alert(item: $pendingPackageUninstall) { pending in
            let package = pending.package
            if let preview = pending.preview {
                let message = packageUninstallAlertMessage(package: package, preview: preview)
                if preview.managerAutomationLevel == "read_only" {
                    return Alert(
                        title: Text(
                            L10n.App.Packages.Alert.uninstallTitle.localized(
                                with: ["package": package.displayName]
                            )
                        ),
                        message: Text(message),
                        dismissButton: .default(Text(L10n.Common.ok.localized))
                    )
                }
                return Alert(
                    title: Text(
                        L10n.App.Packages.Alert.uninstallTitle.localized(
                            with: ["package": package.displayName]
                        )
                    ),
                    message: Text(message),
                    primaryButton: .destructive(Text(L10n.Common.uninstall.localized)) {
                        core.uninstallPackage(package)
                    },
                    secondaryButton: .cancel()
                )
            }

            return Alert(
                title: Text(
                    L10n.App.Packages.Alert.uninstallTitle.localized(
                        with: ["package": package.displayName]
                    )
                ),
                message: Text(
                    L10n.App.Packages.Alert.uninstallMessage.localized(
                        with: [
                            "package": package.displayName,
                            "manager": localizedManagerDisplayName(package.managerId),
                        ]
                    )
                ),
                primaryButton: .destructive(Text(L10n.Common.uninstall.localized)) {
                    core.uninstallPackage(package)
                },
                secondaryButton: .cancel()
            )
        }
    }

    @ViewBuilder
    private func quickActionButtons(for package: PackageItem) -> some View {
        HStack(spacing: 4) {
            if core.canInstallPackage(package) {
                iconActionButton(
                    symbol: "arrow.down.circle",
                    tooltip: L10n.App.Packages.Action.install.localized,
                    enabled: !installActionInFlight(for: package)
                ) {
                    core.installPackage(package)
                }
            }

            if core.canUninstallPackage(package) {
                iconActionButton(
                    symbol: "trash",
                    tooltip: L10n.App.Packages.Action.uninstall.localized,
                    enabled: !core.uninstallActionPackageIds.contains(package.id)
                        && !loadingPackageUninstallPreviewIds.contains(package.id)
                ) {
                    requestPackageUninstallConfirmation(package)
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

    private func installActionInFlight(for package: PackageItem) -> Bool {
        core.isInstallActionInFlight(for: package)
    }

    private func managerBadge(_ text: String) -> some View {
        Text(text)
            .font(.caption2)
            .padding(.horizontal, 4)
            .padding(.vertical, 1)
            .background(
                RoundedRectangle(cornerRadius: 4, style: .continuous)
                    .fill(HelmTheme.surfaceElevated)
                    .overlay(
                        RoundedRectangle(cornerRadius: 4, style: .continuous)
                            .strokeBorder(HelmTheme.borderSubtle.opacity(0.9), lineWidth: 0.8)
                    )
            )
            .foregroundColor(HelmTheme.textSecondary)
    }

    private func requestPackageUninstallConfirmation(_ package: PackageItem) {
        loadingPackageUninstallPreviewIds.insert(package.id)
        core.previewPackageUninstall(package) { preview in
            loadingPackageUninstallPreviewIds.remove(package.id)
            pendingPackageUninstall = PendingPackageUninstall(package: package, preview: preview)
        }
    }

    private func packageUninstallAlertMessage(
        package: PackageItem,
        preview: PackageUninstallPreview
    ) -> String {
        var sections = [
            L10n.App.Packages.Alert.uninstallMessage.localized(
                with: [
                    "package": package.displayName,
                    "manager": localizedManagerDisplayName(package.managerId),
                ]
            )
        ]

        if !preview.summaryLines.isEmpty {
            sections.append(preview.summaryLines.joined(separator: "\n"))
        }

        if !preview.secondaryEffects.isEmpty {
            let effects = preview.secondaryEffects.prefix(3).map { "• \($0)" }
            sections.append(effects.joined(separator: "\n"))
        }

        return sections.joined(separator: "\n\n")
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
    @ObservedObject private var core = HelmCore.shared
    @State private var showSupportOptionsModal = false
    let onClose: () -> Void

    private var buildVersion: String {
        (Bundle.main.object(forInfoDictionaryKey: "CFBundleVersion") as? String)?
            .trimmingCharacters(in: .whitespacesAndNewlines) ?? "-"
    }

    private var helmUpdateDetected: Bool {
        core.outdatedPackages.contains { package in
            let normalized = package.name.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
            return normalized == "helm" || normalized == "helm-cli"
        }
    }

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
                Text(L10n.App.Overlay.About.copyright.localized)
                    .font(.caption2)
                    .foregroundColor(.secondary)
            }

            if helmUpdateDetected {
                Text(L10n.App.Overlay.About.updateDetected.localized)
                    .font(.caption)
                    .foregroundColor(HelmTheme.stateAttention)
                    .fixedSize(horizontal: false, vertical: true)
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
