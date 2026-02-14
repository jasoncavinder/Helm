import SwiftUI

struct SettingsPopoverView: View {
    @ObservedObject var core = HelmCore.shared
    @ObservedObject var localization = LocalizationManager.shared

    @State private var autoCheckEnabled = false
    @State private var checkFrequency = 60
    @State private var showResetConfirmation = false
    @State private var isResetting = false

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(L10n.App.Settings.Tab.title.localized)
                .font(.headline)

            Divider()
            
            // Language Picker
            HStack {
                Text(L10n.App.Settings.Label.language.localized)
                    .font(.subheadline)
                Spacer()
                Picker("", selection: $localization.currentLocale) {
                    Text("\(L10n.App.Settings.Label.systemDefault.localized) (en)").tag("en")
                    // Future: Add other languages
                }
                .labelsHidden()
                .frame(width: 120)
            }
            
            Divider()

            Toggle(L10n.App.Settings.Label.autoCheck.localized, isOn: $autoCheckEnabled)
                .disabled(true)
                .font(.subheadline)

            HStack {
                Text(L10n.App.Settings.Label.checkFrequency.localized)
                    .font(.subheadline)
                    .foregroundColor(.secondary)
                Picker("", selection: $checkFrequency) {
                    Text("15 min").tag(15)
                    Text("30 min").tag(30)
                    Text("1 hour").tag(60)
                    Text("Daily").tag(1440)
                }
                .labelsHidden()
                .disabled(true)
                .frame(width: 100)
            }

            Divider()

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

            Button(action: {}) {
                HStack {
                    Image(systemName: "arrow.up.square")
                    Text(L10n.App.Settings.Action.upgradeAll.localized)
                }
                .frame(maxWidth: .infinity)
            }
            .disabled(true)
            .help("Upgrade all not yet implemented")

            Divider()

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
        .padding(16)
        .frame(width: 240) // Slightly wider for language picker
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
