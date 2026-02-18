import SwiftUI

@main
struct HelmRedesignApp: App {
    @NSApplicationDelegateAdaptor(HelmRedesignAppDelegate.self) private var appDelegate
    @StateObject private var store = AppStateStore()

    var body: some Scene {
        MenuBarExtra {
            StatusPopoverView()
                .environmentObject(store)
        } label: {
            Image(systemName: store.snapshot.aggregateStatus.symbolName)
                .accessibilityLabel(Text("app.name"))
        }

        WindowGroup(id: "control-center") {
            ControlCenterWindowView()
                .environmentObject(store)
        }
        .defaultSize(width: 1100, height: 720)

        Settings {
            SettingsSectionView()
                .environmentObject(store)
        }
    }
}
