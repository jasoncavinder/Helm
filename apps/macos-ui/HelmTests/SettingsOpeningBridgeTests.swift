import SwiftUI
import XCTest

final class HelmSettingsOpenRouterTests: XCTestCase {
    func testPopoverRemainsVisibleWhenHelmDeactivates() {
        XCTAssertFalse(HelmPanelDeactivationPolicy.popoverHidesOnDeactivate)
    }

    func testSettingsPanelRemainsVisibleWhenHelmDeactivates() {
        XCTAssertFalse(HelmPanelDeactivationPolicy.settingsHidesOnDeactivate)
    }

    func testSettingsPanelPolicyCanJoinExternalFullScreenWithoutFollowingEverySpace() {
        let behavior = HelmSettingsPanelPolicy.collectionBehavior

        XCTAssertTrue(behavior.contains(.moveToActiveSpace))
        XCTAssertTrue(behavior.contains(.transient))
        XCTAssertTrue(behavior.contains(.canJoinAllApplications))
        XCTAssertTrue(behavior.contains(.fullScreenAuxiliary))
        XCTAssertFalse(behavior.contains(.canJoinAllSpaces))
        XCTAssertFalse(behavior.contains(.primary))
        XCTAssertFalse(behavior.contains(.auxiliary))
    }

    func testSettingsPanelAllowsFullHeightNativeSidebarLayout() {
        XCTAssertTrue(HelmSettingsPanelPolicy.styleMask.contains(.fullSizeContentView))
    }

    func testPrimaryWindowsHideRedundantVisibleTitles() {
        XCTAssertEqual(HelmWindowChromePolicy.titleVisibility, .hidden)
    }

    func testPrimaryWindowTitlesRefreshForAccessibilityAfterLocaleChange() {
        let dashboardWindow = NSWindow()
        let settingsWindow = NSWindow()
        dashboardWindow.title = "Helm Dashboard"
        settingsWindow.title = "Helm Settings"

        HelmPrimaryWindowTitlePolicy.apply(
            dashboardTitle: "Helm vezérlőpult",
            settingsTitle: "Helm beállításai",
            dashboardWindow: dashboardWindow,
            settingsWindow: settingsWindow
        )

        XCTAssertEqual(dashboardWindow.title, "Helm vezérlőpult")
        XCTAssertEqual(settingsWindow.title, "Helm beállításai")
        XCTAssertEqual(dashboardWindow.accessibilityTitle(), "Helm vezérlőpult")
        XCTAssertEqual(settingsWindow.accessibilityTitle(), "Helm beállításai")
    }

    func testPrimaryWindowFramesRemainOwnedByAppKit() {
        let controller = NSHostingController(rootView: EmptyView())

        HelmHostingSizingPolicy.apply(to: controller)

        XCTAssertTrue(controller.sizingOptions.isEmpty)
        XCTAssertEqual(
            controller.view.contentCompressionResistancePriority(for: .horizontal),
            .defaultLow
        )
        XCTAssertEqual(
            controller.view.contentCompressionResistancePriority(for: .vertical),
            .defaultLow
        )
    }

    func testFirstRunUsesSmallerFixedWindowBeforeRestoringDashboardSizing() {
        let window = NSWindow(
            contentRect: NSRect(
                origin: .zero,
                size: HelmPrimaryWindowSizingPolicy.dashboardDefaultSize
            ),
            styleMask: [.titled, .closable, .resizable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )

        HelmPrimaryWindowSizingPolicy.applyFirstRun(to: window)

        XCTAssertFalse(window.styleMask.contains(.resizable))
        XCTAssertEqual(window.frame.size, HelmPrimaryWindowSizingPolicy.firstRunSize)
        XCTAssertEqual(window.minSize, HelmPrimaryWindowSizingPolicy.firstRunSize)
        XCTAssertEqual(window.maxSize, HelmPrimaryWindowSizingPolicy.firstRunSize)
        XCTAssertLessThan(
            HelmPrimaryWindowSizingPolicy.firstRunSize.width,
            HelmPrimaryWindowSizingPolicy.dashboardMinimumSize.width
        )

        HelmPrimaryWindowSizingPolicy.applyDashboard(to: window)

        XCTAssertTrue(window.styleMask.contains(.resizable))
        XCTAssertEqual(window.minSize, HelmPrimaryWindowSizingPolicy.dashboardMinimumSize)
    }

    func testPrimaryWindowResizePoliciesClampRequestedGeometry() {
        XCTAssertEqual(
            HelmPrimaryWindowSizingPolicy.dashboardResizeSize(
                NSSize(width: 700, height: 400)
            ),
            HelmPrimaryWindowSizingPolicy.dashboardMinimumSize
        )
        XCTAssertEqual(
            HelmPrimaryWindowSizingPolicy.settingsResizeSize(
                NSSize(width: 900, height: 800)
            ),
            HelmPrimaryWindowSizingPolicy.settingsMaximumSize
        )
    }

    func testRestoredDashboardFrameKeepsInspectorEdgeOnscreen() {
        let visibleFrame = NSRect(x: 0, y: 25, width: 1728, height: 1067)
        let restoredFrame = NSRect(x: 863, y: 304, width: 1024, height: 640)

        let constrained = HelmPrimaryWindowSizingPolicy.fullyVisibleFrame(
            restoredFrame,
            in: visibleFrame
        )

        XCTAssertEqual(constrained.origin.x, 704)
        XCTAssertEqual(constrained.origin.y, restoredFrame.origin.y)
        XCTAssertLessThanOrEqual(constrained.maxX, visibleFrame.maxX)
        XCTAssertLessThanOrEqual(constrained.maxY, visibleFrame.maxY)
    }

    func testOversizedRestoredDashboardFrameShrinksToVisibleScreen() {
        let visibleFrame = NSRect(x: -1440, y: 25, width: 1440, height: 877)
        let restoredFrame = NSRect(x: 80, y: 120, width: 2000, height: 1200)

        let constrained = HelmPrimaryWindowSizingPolicy.fullyVisibleFrame(
            restoredFrame,
            in: visibleFrame,
            minimumSize: HelmPrimaryWindowSizingPolicy.dashboardMinimumSize
        )

        XCTAssertEqual(constrained.size, visibleFrame.size)
        XCTAssertGreaterThanOrEqual(constrained.minX, visibleFrame.minX)
        XCTAssertGreaterThanOrEqual(constrained.minY, visibleFrame.minY)
        XCTAssertLessThanOrEqual(constrained.maxX, visibleFrame.maxX)
        XCTAssertLessThanOrEqual(constrained.maxY, visibleFrame.maxY)
    }

    func testRestoredDashboardFramePreservesMinimumOnSmallerScreen() {
        let visibleFrame = NSRect(x: 0, y: 25, width: 900, height: 590)
        let restoredFrame = NSRect(x: 200, y: 200, width: 1600, height: 1000)

        let constrained = HelmPrimaryWindowSizingPolicy.fullyVisibleFrame(
            restoredFrame,
            in: visibleFrame,
            minimumSize: HelmPrimaryWindowSizingPolicy.dashboardMinimumSize
        )

        XCTAssertEqual(
            constrained.size,
            HelmPrimaryWindowSizingPolicy.dashboardMinimumSize
        )
        XCTAssertEqual(constrained.origin, visibleFrame.origin)
    }

    func testSettingsPanelPresentationDetachesFromDashboardAfterOrdering() {
        let dashboardWindow = NSWindow()
        let settingsWindow = NSWindow()
        var parentDuringPresentation: NSWindow?

        HelmSettingsPanelPolicy.presentIndependently(
            settingsWindow: settingsWindow,
            above: dashboardWindow,
            present: { window in
                parentDuringPresentation = window.parent
            }
        )

        XCTAssertTrue(parentDuringPresentation === dashboardWindow)
        XCTAssertNil(settingsWindow.parent)
    }

    func testDetachingFromDifferentWindowPreservesCurrentParent() {
        let dashboardWindow = NSWindow()
        let otherWindow = NSWindow()
        let settingsWindow = NSWindow()
        dashboardWindow.addChildWindow(settingsWindow, ordered: .above)

        HelmSettingsPanelPolicy.detachSettingsWindowFromDashboard(
            settingsWindow: settingsWindow,
            dashboardWindow: otherWindow
        )

        XCTAssertTrue(settingsWindow.parent === dashboardWindow)
    }

    func testRequestOpenUsesConfiguredAppKitWindowAction() {
        var firstOpenCount = 0
        var configuredOpenCount = 0
        let router = HelmSettingsOpenRouter {
            firstOpenCount += 1
        }

        router.requestOpen()
        router.configure {
            configuredOpenCount += 1
        }
        router.requestOpen()

        XCTAssertEqual(firstOpenCount, 1)
        XCTAssertEqual(configuredOpenCount, 1)
    }

    func testRequestOpenCanRouteDirectlyToSupportPane() {
        var openCount = 0
        let router = HelmSettingsOpenRouter {
            openCount += 1
        }

        router.requestOpen(pane: .support)

        XCTAssertEqual(openCount, 1)
        XCTAssertEqual(router.requestedPane, .support)
        XCTAssertEqual(router.paneRequestToken, 1)

        router.requestOpen()

        XCTAssertEqual(openCount, 2)
        XCTAssertNil(router.requestedPane)
        XCTAssertEqual(router.paneRequestToken, 2)
    }
}
