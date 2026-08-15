import AppKit

enum HelmPanelDeactivationPolicy {
    static let popoverHidesOnDeactivate = false
    static let settingsHidesOnDeactivate = false
}

enum HelmSettingsPanelPolicy {
    static let titleVisibility: NSWindow.TitleVisibility = .hidden
    static let styleMask: NSWindow.StyleMask = [
        .titled,
        .closable,
        .resizable,
        .fullSizeContentView
    ]
    static let collectionBehavior: NSWindow.CollectionBehavior = [
        .moveToActiveSpace,
        .transient,
        .canJoinAllApplications,
        .fullScreenAuxiliary
    ]

    static func detachSettingsWindowFromClosingDashboard(
        settingsWindow: NSWindow?,
        dashboardWindow: NSWindow?
    ) {
        guard let settingsWindow,
              let dashboardWindow,
              settingsWindow.parent == dashboardWindow else {
            return
        }
        dashboardWindow.removeChildWindow(settingsWindow)
        if settingsWindow.isVisible {
            settingsWindow.orderFront(nil)
        }
    }
}

final class HelmSettingsOpenRouter {
    typealias OpenAction = () -> Void

    private var openAction: OpenAction

    init(openAction: @escaping OpenAction = {}) {
        self.openAction = openAction
    }

    func configure(openAction: @escaping OpenAction) {
        self.openAction = openAction
    }

    func requestOpen() {
        openAction()
    }
}
