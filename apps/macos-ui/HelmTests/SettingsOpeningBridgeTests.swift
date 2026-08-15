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

    func testSettingsPanelUsesToolbarTitleInsteadOfDuplicateWindowTitle() {
        XCTAssertEqual(HelmSettingsPanelPolicy.titleVisibility, .hidden)
        XCTAssertTrue(HelmSettingsPanelPolicy.styleMask.contains(.fullSizeContentView))
    }

    func testClosingDashboardDetachesSettingsPanelFromParentWindow() {
        let dashboardWindow = NSWindow()
        let settingsWindow = NSWindow()
        dashboardWindow.addChildWindow(settingsWindow, ordered: .above)

        HelmSettingsPanelPolicy.detachSettingsWindowFromClosingDashboard(
            settingsWindow: settingsWindow,
            dashboardWindow: dashboardWindow
        )

        XCTAssertNil(settingsWindow.parent)
    }

    func testClosingDifferentWindowDoesNotDetachSettingsPanel() {
        let dashboardWindow = NSWindow()
        let otherWindow = NSWindow()
        let settingsWindow = NSWindow()
        dashboardWindow.addChildWindow(settingsWindow, ordered: .above)

        HelmSettingsPanelPolicy.detachSettingsWindowFromClosingDashboard(
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
}
