import XCTest

final class HelmSettingsOpenRouterTests: XCTestCase {
    func testPopoverRemainsVisibleWhenHelmDeactivates() {
        XCTAssertFalse(HelmPanelDeactivationPolicy.popoverHidesOnDeactivate)
    }

    func testSettingsPanelHidesWhenHelmDeactivates() {
        XCTAssertTrue(HelmPanelDeactivationPolicy.settingsHidesOnDeactivate)
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
