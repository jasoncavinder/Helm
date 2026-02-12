import SwiftUI

struct SettingsPopoverView: View {
    @ObservedObject var core = HelmCore.shared

    @State private var autoCheckEnabled = false
    @State private var checkFrequency = 60

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

            Button(action: {}) {
                HStack {
                    Image(systemName: "arrow.up.square")
                    Text("Upgrade All")
                }
                .frame(maxWidth: .infinity)
            }
            .disabled(true)
            .help("Upgrade all not yet implemented")

            Divider()

            Button(action: {
                NSApplication.shared.terminate(nil)
            }) {
                HStack {
                    Image(systemName: "power")
                    Text("Quit Helm")
                }
                .frame(maxWidth: .infinity)
            }
        }
        .padding(16)
        .frame(width: 220)
    }
}
