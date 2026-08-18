import AppKit
import Combine
import SwiftUI

enum HelmPanelDeactivationPolicy {
    static let popoverHidesOnDeactivate = false
    static let settingsHidesOnDeactivate = false
}

enum HelmWindowChromePolicy {
    static let titleVisibility: NSWindow.TitleVisibility = .hidden
}

enum HelmHostingSizingPolicy {
    static let windowOwned: NSHostingSizingOptions = []

    static func apply<Content: View>(to controller: NSHostingController<Content>) {
        controller.sizingOptions = windowOwned
        controller.view.setContentHuggingPriority(.defaultLow, for: .horizontal)
        controller.view.setContentHuggingPriority(.defaultLow, for: .vertical)
        controller.view.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        controller.view.setContentCompressionResistancePriority(.defaultLow, for: .vertical)
    }
}

enum HelmSettingsPanelPolicy {
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

final class HelmSettingsOpenRouter: ObservableObject {
    typealias OpenAction = () -> Void

    private var openAction: OpenAction
    @Published private(set) var requestedPane: SettingsPane?
    @Published private(set) var paneRequestToken: Int = 0

    init(openAction: @escaping OpenAction = {}) {
        self.openAction = openAction
    }

    func configure(openAction: @escaping OpenAction) {
        self.openAction = openAction
    }

    func requestOpen(pane: SettingsPane? = nil) {
        requestedPane = pane
        paneRequestToken &+= 1
        openAction()
    }
}
