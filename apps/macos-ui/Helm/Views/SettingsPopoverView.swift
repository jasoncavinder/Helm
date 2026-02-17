import SwiftUI

struct SettingsPopoverView: View {
    @ObservedObject var core = HelmCore.shared
    @ObservedObject var localization = LocalizationManager.shared

    @State private var autoCheckEnabled = false
    @State private var checkFrequency = 60
    @State private var showResetConfirmation = false
    @State private var showUpgradeConfirmation = false
    @State private var isResetting = false

    private var upgradePreviewSummary: String {
        let noOsCount = core.upgradeAllPreviewCount(includePinned: false, allowOsUpdates: false)
        let noOsLine = "\(L10n.App.Settings.Alert.UpgradeAll.upgradeNoOs.localized): \(L10n.App.Managers.Label.packageCount.localized(with: ["count": noOsCount]))"
        let noOsBreakdown = core
            .upgradeAllPreviewBreakdown(includePinned: false, allowOsUpdates: false)
            .prefix(3)
            .map { entry in
                "\(entry.manager): \(L10n.App.Managers.Label.packageCount.localized(with: ["count": entry.count]))"
            }
            .joined(separator: "\n")

        guard !core.safeModeEnabled else {
            return noOsBreakdown.isEmpty ? noOsLine : "\(noOsLine)\n\(noOsBreakdown)"
        }

        let withOsCount = core.upgradeAllPreviewCount(includePinned: false, allowOsUpdates: true)
        let withOsLine = "\(L10n.App.Settings.Alert.UpgradeAll.upgradeWithOs.localized): \(L10n.App.Managers.Label.packageCount.localized(with: ["count": withOsCount]))"
        let withOsBreakdown = core
            .upgradeAllPreviewBreakdown(includePinned: false, allowOsUpdates: true)
            .prefix(3)
            .map { entry in
                "\(entry.manager): \(L10n.App.Managers.Label.packageCount.localized(with: ["count": entry.count]))"
            }
            .joined(separator: "\n")

        let noOsSection = noOsBreakdown.isEmpty ? noOsLine : "\(noOsLine)\n\(noOsBreakdown)"
        let withOsSection = withOsBreakdown.isEmpty ? withOsLine : "\(withOsLine)\n\(withOsBreakdown)"
        return "\(noOsSection)\n\n\(withOsSection)"
    }

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
                    showUpgradeConfirmation = true
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
        .alert(L10n.App.Settings.Alert.UpgradeAll.title.localized, isPresented: $showUpgradeConfirmation) {
            Button(L10n.App.Settings.Alert.UpgradeAll.upgradeNoOs.localized) {
                core.upgradeAll(includePinned: false, allowOsUpdates: false)
            }
            if !core.safeModeEnabled {
                Button(L10n.App.Settings.Alert.UpgradeAll.upgradeWithOs.localized, role: .destructive) {
                    core.upgradeAll(includePinned: false, allowOsUpdates: true)
                }
            }
            Button(L10n.Common.cancel.localized, role: .cancel) {}
        } message: {
            if core.safeModeEnabled {
                Text("\(L10n.App.Settings.Alert.UpgradeAll.safeModeMessage.localized)\n\n\(upgradePreviewSummary)")
            } else {
                Text("\(L10n.App.Settings.Alert.UpgradeAll.standardMessage.localized)\n\n\(upgradePreviewSummary)")
            }
        }
    }
}
