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

enum HelmPrimaryWindowSizingPolicy {
    static let dashboardDefaultSize = NSSize(width: 1120, height: 740)
    static let dashboardMinimumSize = NSSize(width: 1024, height: 640)
    static let firstRunSize = NSSize(width: 960, height: 600)
    static let settingsDefaultSize = NSSize(width: 680, height: 500)
    static let settingsMinimumSize = NSSize(width: 600, height: 420)
    static let settingsMaximumSize = NSSize(width: 760, height: 600)

    private static let unconstrainedMaximumSize = NSSize(
        width: CGFloat.greatestFiniteMagnitude,
        height: CGFloat.greatestFiniteMagnitude
    )

    static func applyDashboard(to window: NSWindow) {
        window.styleMask.insert(.resizable)
        applyBounds(
            minimum: dashboardMinimumSize,
            maximum: unconstrainedMaximumSize,
            to: window
        )
        constrainToVisibleScreen(window)
        window.standardWindowButton(.zoomButton)?.isEnabled = true
    }

    static func applyFirstRun(to window: NSWindow) {
        window.styleMask.remove(.resizable)
        window.contentMinSize = .zero
        window.contentMaxSize = unconstrainedMaximumSize
        window.minSize = .zero
        window.maxSize = unconstrainedMaximumSize
        window.setContentSize(firstRunSize)
        applyBounds(minimum: firstRunSize, maximum: firstRunSize, to: window)
        window.standardWindowButton(.zoomButton)?.isEnabled = false
    }

    static func applySettings(to window: NSWindow) {
        applyBounds(
            minimum: settingsMinimumSize,
            maximum: settingsMaximumSize,
            to: window
        )
        constrainToVisibleScreen(window)
    }

    static func dashboardResizeSize(_ proposedSize: NSSize) -> NSSize {
        constrainedSize(
            proposedSize,
            minimum: dashboardMinimumSize,
            maximum: unconstrainedMaximumSize
        )
    }

    static func settingsResizeSize(_ proposedSize: NSSize) -> NSSize {
        constrainedSize(
            proposedSize,
            minimum: settingsMinimumSize,
            maximum: settingsMaximumSize
        )
    }

    static func constrainedSize(
        _ proposedSize: NSSize,
        minimum: NSSize,
        maximum: NSSize
    ) -> NSSize {
        NSSize(
            width: min(max(proposedSize.width, minimum.width), maximum.width),
            height: min(max(proposedSize.height, minimum.height), maximum.height)
        )
    }

    static func fullyVisibleFrame(_ frame: NSRect, in visibleFrame: NSRect) -> NSRect {
        let maximumX = max(visibleFrame.minX, visibleFrame.maxX - frame.width)
        let maximumY = max(visibleFrame.minY, visibleFrame.maxY - frame.height)
        var constrained = frame
        constrained.origin.x = min(max(frame.minX, visibleFrame.minX), maximumX)
        constrained.origin.y = min(max(frame.minY, visibleFrame.minY), maximumY)
        return constrained
    }

    private static func applyBounds(
        minimum: NSSize,
        maximum: NSSize,
        to window: NSWindow
    ) {
        window.contentMinSize = minimum
        window.contentMaxSize = maximum
        window.minSize = minimum
        window.maxSize = maximum

        let constrained = constrainedSize(
            window.frame.size,
            minimum: minimum,
            maximum: maximum
        )
        guard constrained != window.frame.size else { return }

        var frame = window.frame
        frame.origin.y = frame.maxY - constrained.height
        frame.size = constrained
        window.setFrame(frame, display: false)
    }

    private static func constrainToVisibleScreen(_ window: NSWindow) {
        guard let screen = window.screen ?? NSScreen.main else { return }
        let constrainedFrame = fullyVisibleFrame(window.frame, in: screen.visibleFrame)
        guard constrainedFrame != window.frame else { return }
        window.setFrame(constrainedFrame, display: false)
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
