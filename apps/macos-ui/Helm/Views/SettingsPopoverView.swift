import SwiftUI

struct SettingsPopoverView: View {
    @ObservedObject var core = HelmCore.shared

    @State private var autoCheckEnabled = false
    @State private var checkFrequency = 60
    @State private var showResetConfirmation = false
    @State private var showUpgradeConfirmation = false
    @State private var isResetting = false

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Settings")
                .font(.headline)

            Divider()

            Toggle("Auto-check for updates", isOn: $autoCheckEnabled)
                .disabled(true)
                .font(.subheadline)

            HStack {
                Text("Check every")
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

            Toggle("Safe Mode (block macOS updates)", isOn: Binding(
                get: { core.safeModeEnabled },
                set: { core.setSafeMode($0) }
            ))
            .font(.subheadline)

            Toggle("Auto-clean old Homebrew kegs", isOn: Binding(
                get: { core.homebrewKegAutoCleanupEnabled },
                set: { core.setHomebrewKegAutoCleanup($0) }
            ))
            .font(.subheadline)

            Divider()

            Button(action: {
                core.triggerRefresh()
            }) {
                HStack {
                    Image(systemName: "arrow.clockwise")
                    Text("Refresh Now")
                }
                .frame(maxWidth: .infinity)
            }
            .disabled(core.isRefreshing)

            Button(action: {
                showUpgradeConfirmation = true
            }) {
                HStack {
                    Image(systemName: "arrow.up.square")
                    Text("Upgrade All")
                }
                .frame(maxWidth: .infinity)
            }
            .disabled(core.isRefreshing || isResetting)

            Group {
                Divider()

                Button(action: {
                    showResetConfirmation = true
                }) {
                    HStack {
                        Image(systemName: "arrow.counterclockwise")
                        Text("Reset Local Data")
                    }
                    .foregroundColor(.red)
                    .frame(maxWidth: .infinity)
                }
                .disabled(core.isRefreshing || isResetting)

                Button {
                    NSApplication.shared.terminate(nil)
                } label: {
                    HStack {
                        Image(systemName: "power")
                        Text("Quit Helm")
                    }
                    .frame(maxWidth: .infinity)
                }
            }
        }
        .padding(16)
        .frame(width: 220)
        .alert("Reset Local Data?", isPresented: $showResetConfirmation) {
            Button("Cancel", role: .cancel) {}
            Button("Reset", role: .destructive) {
                isResetting = true
                core.resetDatabase { _ in
                    isResetting = false
                }
            }
        } message: {
            Text("This will clear all cached data and return Helm to its initial state. Your installed packages will not be affected.")
        }
        .alert("Upgrade All Packages?", isPresented: $showUpgradeConfirmation) {
            Button("Upgrade (No OS Updates)") {
                core.upgradeAll(includePinned: false, allowOsUpdates: false)
            }
            if !core.safeModeEnabled {
                Button("Upgrade Including OS Updates", role: .destructive) {
                    core.upgradeAll(includePinned: false, allowOsUpdates: true)
                }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            if core.safeModeEnabled {
                Text("Safe Mode is enabled, so macOS software updates will be blocked. Pinned packages are excluded.")
            } else {
                Text("Pinned packages are excluded. Choose the OS update option only when you explicitly want softwareupdate to run.")
            }
        }
    }
}
