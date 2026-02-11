import SwiftUI

@main
struct HelmApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) var appDelegate
    @StateObject var core = HelmCore.shared

    var body: some Scene {
        Settings {
            EmptyView()
        }
    }
}
