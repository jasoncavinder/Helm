import SwiftUI

struct SettingsPopoverView: View {
    @ObservedObject var core = HelmCore.shared
    @ObservedObject var localization = LocalizationManager.shared

    @State private var autoCheckEnabled = false
    @State private var checkFrequency = 60
    @State private var showResetConfirmation = false
    @State private var showUpgradePreview = false
    @State private var isResetting = false

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Group {
                Text(L10n.App.Settings.Tab.title.localized)
                    .font(.headline)

                Divider()
                
                // Language Picker
                HStack {
                    Text(L10n.App.Settings.Label.language.localized)
                        .font(.subheadline)
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
                    .frame(width: 260)
                }
                
                Divider()
            }

            Group {
                Toggle(L10n.App.Settings.Label.autoCheck.localized, isOn: $autoCheckEnabled)
                    .disabled(true)
                    .font(.subheadline)

                HStack {
                    Text(L10n.App.Settings.Label.checkFrequency.localized)
                        .font(.subheadline)
                        .foregroundColor(.secondary)
                    Picker("", selection: $checkFrequency) {
                        Text(L10n.App.Settings.Frequency.every15Min.localized).tag(15)
                        Text(L10n.App.Settings.Frequency.every30Min.localized).tag(30)
                        Text(L10n.App.Settings.Frequency.every1Hour.localized).tag(60)
                        Text(L10n.App.Settings.Frequency.daily.localized).tag(1440)
                    }
                    .labelsHidden()
                    .disabled(true)
                    .frame(width: 100)
                }
                
                // Safe Mode Toggle (Merged from 0.8)
                Toggle(L10n.App.Settings.Label.safeMode.localized, isOn: Binding(
                    get: { core.safeModeEnabled },
                    set: { core.setSafeMode($0) }
                ))
                .font(.subheadline)

                // Homebrew Cleanup (Merged from 0.8)
                Toggle(L10n.App.Settings.Label.autoCleanKegs.localized, isOn: Binding(
                    get: { core.homebrewKegAutoCleanupEnabled },
                    set: { core.setHomebrewKegAutoCleanup($0) }
                ))
                .font(.subheadline)

                Divider()
            }

            Group {
                Button(action: {
                    core.triggerRefresh()
                }) {
                    HStack {
                        Image(systemName: "arrow.clockwise")
                        Text(L10n.App.Settings.Action.refreshNow.localized)
                    }
                    .frame(maxWidth: .infinity)
                }
                .disabled(core.isRefreshing)

                Button(action: {
                    showUpgradePreview = true
                }) {
                    HStack {
                        Image(systemName: "arrow.up.square")
                        Text(L10n.App.Settings.Action.upgradeAll.localized)
                    }
                    .frame(maxWidth: .infinity)
                }
                .disabled(core.isRefreshing || isResetting)

                Divider()
            }

            Group {
                Button(action: {
                    showResetConfirmation = true
                }) {
                    HStack {
                        Image(systemName: "arrow.counterclockwise")
                        Text(L10n.App.Settings.Action.reset.localized)
                    }
                    .foregroundColor(.red)
                    .frame(maxWidth: .infinity)
                }
                .disabled(core.isRefreshing || isResetting)

                Button(action: {
                    NSApplication.shared.terminate(nil)
                }) {
                    HStack {
                        Image(systemName: "power")
                        Text(L10n.App.Settings.Action.quit.localized)
                    }
                    .frame(maxWidth: .infinity)
                }
            }
        }
        .padding(16)
        .frame(width: 440)
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
        .sheet(isPresented: $showUpgradePreview) {
            UpgradePreviewSheetView(isPresented: $showUpgradePreview)
        }
    }
}

private struct UpgradePreviewSheetView: View {
    @ObservedObject var core = HelmCore.shared
    @Binding var isPresented: Bool

    private var noOsCount: Int {
        core.upgradeAllPreviewCount(includePinned: false, allowOsUpdates: false)
    }

    private var noOsBreakdown: [UpgradePreviewPlanner.Entry] {
        core.upgradeAllPreviewBreakdown(includePinned: false, allowOsUpdates: false)
    }

    private var withOsCount: Int {
        core.upgradeAllPreviewCount(includePinned: false, allowOsUpdates: true)
    }

    private var withOsBreakdown: [UpgradePreviewPlanner.Entry] {
        core.upgradeAllPreviewBreakdown(includePinned: false, allowOsUpdates: true)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(L10n.App.Settings.Alert.UpgradeAll.title.localized)
                .font(.headline)

            Text(
                core.safeModeEnabled
                    ? L10n.App.Settings.Alert.UpgradeAll.safeModeMessage.localized
                    : L10n.App.Settings.Alert.UpgradeAll.standardMessage.localized
            )
            .font(.subheadline)
            .foregroundColor(.secondary)

            Divider()

            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    UpgradePreviewSectionView(
                        title: L10n.App.Settings.Alert.UpgradeAll.upgradeNoOs.localized,
                        count: noOsCount,
                        entries: noOsBreakdown
                    )

                    if !core.safeModeEnabled {
                        Divider()
                        UpgradePreviewSectionView(
                            title: L10n.App.Settings.Alert.UpgradeAll.upgradeWithOs.localized,
                            count: withOsCount,
                            entries: withOsBreakdown
                        )
                    }
                }
            }

            Divider()

            HStack(spacing: 10) {
                Button(L10n.Common.cancel.localized, role: .cancel) {
                    isPresented = false
                }

                Spacer()

                Button(L10n.App.Settings.Alert.UpgradeAll.upgradeNoOs.localized) {
                    core.upgradeAll(includePinned: false, allowOsUpdates: false)
                    isPresented = false
                }
                .buttonStyle(.borderedProminent)

                if !core.safeModeEnabled {
                    Button(L10n.App.Settings.Alert.UpgradeAll.upgradeWithOs.localized, role: .destructive) {
                        core.upgradeAll(includePinned: false, allowOsUpdates: true)
                        isPresented = false
                    }
                    .buttonStyle(.bordered)
                }
            }
        }
        .padding(16)
        .frame(width: 460, height: 420)
    }
}

private struct UpgradePreviewSectionView: View {
    let title: String
    let count: Int
    let entries: [UpgradePreviewPlanner.Entry]

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title)
                .font(.subheadline)
                .fontWeight(.semibold)

            Text(L10n.App.Managers.Label.packageCount.localized(with: ["count": count]))
                .font(.caption)
                .foregroundColor(.secondary)

            if entries.isEmpty {
                Text(L10n.App.Packages.State.noPackagesFound.localized)
                    .font(.caption)
                    .foregroundColor(.secondary)
            } else {
                ForEach(Array(entries.prefix(8)), id: \.manager) { entry in
                    HStack(spacing: 6) {
                        Text(entry.manager)
                            .font(.caption)
                        Text("·")
                            .font(.caption)
                            .foregroundColor(.secondary)
                        Text(L10n.App.Managers.Label.packageCount.localized(with: ["count": entry.count]))
                            .font(.caption)
                            .foregroundColor(.secondary)
                    }
                }
            }
        }
    }
}
