import SwiftUI

@main
struct HelmApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) var appDelegate
    @StateObject var core = HelmCore.shared

    var body: some Scene {
        Settings {
            EmptyView()
        }
        .commands {
            CommandGroup(replacing: .newItem) {
                Button(L10n.App.Action.openControlCenter.localized) {
                    appDelegate.openDashboardFromApplicationMenu()
                }
                .keyboardShortcut("1", modifiers: .command)
            }

            CommandGroup(replacing: .appSettings) {
                Button(L10n.Common.settings.localized) {
                    appDelegate.openSettingsFromApplicationMenu()
                }
                .keyboardShortcut(",", modifiers: .command)
            }
        }
    }
}
