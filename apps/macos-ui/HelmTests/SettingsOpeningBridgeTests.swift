import XCTest

final class HelmSettingsOpenRouterTests: XCTestCase {
    func testRegisteredOpenUsesMostRecentLiveBridge() {
        let router = HelmSettingsOpenRouter()
        var firstOpenCount = 0
        var secondOpenCount = 0
        let firstID = router.register { firstOpenCount += 1 }
        let secondID = router.register { secondOpenCount += 1 }

        router.requestRegisteredOpen()
        XCTAssertEqual(firstOpenCount, 0)
        XCTAssertEqual(secondOpenCount, 1)

        router.unregister(secondID)
        router.requestRegisteredOpen()
        XCTAssertEqual(firstOpenCount, 1)
        XCTAssertEqual(secondOpenCount, 1)
        router.unregister(firstID)
    }

    func testRequestBeforeBridgeAttachmentIsDeliveredOnce() {
        let router = HelmSettingsOpenRouter()
        var openCount = 0
        let openExpectation = expectation(description: "Deferred Settings request opens")

        router.requestRegisteredOpen()
        let registrationID = router.register {
            openCount += 1
            openExpectation.fulfill()
        }
        wait(for: [openExpectation], timeout: 1)

        XCTAssertEqual(openCount, 1)
        router.unregister(registrationID)
    }
}
