import SwiftUI

struct SettingsSectionView: View {
    @AppStorage("helm.settings.autoCheck") private var autoCheck = true
    @AppStorage("helm.settings.checkOnLaunch") private var checkOnLaunch = true
    @AppStorage("helm.settings.autoApply") private var autoApply = false
    @AppStorage("helm.settings.reduceMotion") private var reduceMotion = false

    var body: some View {
        Form {
            Section {
                Toggle("settings.autoCheck", isOn: $autoCheck)
                Toggle("settings.checkOnLaunch", isOn: $checkOnLaunch)
                Toggle("settings.autoApply", isOn: $autoApply)
            } header: {
                Text("settings.updates")
            }

            Section {
                Toggle("settings.reduceMotion", isOn: $reduceMotion)
            } header: {
                Text("settings.accessibility")
            }
        }
        .padding(20)
    }
}
