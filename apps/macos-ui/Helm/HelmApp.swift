import SwiftUI

@main
struct HelmApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) var appDelegate
    @StateObject var core = HelmCore.shared

    var body: some Scene {
        Settings {
            SettingsWindowView()
                .environmentObject(appDelegate.controlCenterContext)
        }
        .commands {
            HelmApplicationCommands(
                appDelegate: appDelegate,
                context: appDelegate.controlCenterContext,
                core: core
            )
        }
    }
}

private struct HelmApplicationCommands: Commands {
    let appDelegate: AppDelegate
    @ObservedObject var context: ControlCenterContext
    @ObservedObject var core: HelmCore

    var body: some Commands {
        CommandGroup(replacing: .newItem) {
            Button(L10n.App.Action.openControlCenter.localized) {
                appDelegate.openDashboardFromApplicationMenu()
            }
            .keyboardShortcut("1", modifiers: .command)
        }

        CommandGroup(after: .textEditing) {
            Button("app.command.find_software".localized) {
                appDelegate.focusSearchFromApplicationMenu()
            }
            .keyboardShortcut("f", modifiers: .command)
            .disabled(!core.hasCompletedOnboarding)
        }

        CommandGroup(after: .toolbar) {
            Divider()

            Button(
                context.isSidebarVisible
                    ? "app.command.hide_sidebar".localized
                    : "app.command.show_sidebar".localized
            ) {
                appDelegate.toggleSidebarFromApplicationMenu()
            }
            .keyboardShortcut("s", modifiers: [.command, .control])
            .disabled(!core.hasCompletedOnboarding)

            Button(
                context.isInspectorVisible
                    ? "app.command.hide_inspector".localized
                    : "app.command.show_inspector".localized
            ) {
                appDelegate.toggleInspectorFromApplicationMenu()
            }
            .keyboardShortcut("i", modifiers: [.command, .control])
            .disabled(
                !core.hasCompletedOnboarding
                    || !(context.selectedSection ?? .overview).supportsInspector
            )

            Divider()

            ForEach(ControlCenterSection.wayfinderWorkspaces) { section in
                Button(section.title) {
                    appDelegate.selectSectionFromApplicationMenu(section)
                }
                .disabled(!core.hasCompletedOnboarding)
            }

            Button(ControlCenterSection.managers.title) {
                appDelegate.selectSectionFromApplicationMenu(.managers)
            }
            .disabled(!core.hasCompletedOnboarding)

            Divider()

            Button(L10n.Common.refresh.localized) {
                appDelegate.refreshFromApplicationMenu()
            }
            .keyboardShortcut("r", modifiers: .command)
            .disabled(!core.hasCompletedOnboarding || core.isRefreshing)
        }
    }
}
