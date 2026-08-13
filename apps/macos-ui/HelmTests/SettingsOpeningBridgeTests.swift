import XCTest

final class HelmSettingsOpenRouterTests: XCTestCase {
    func testSettingsWindowPolicyMovesToActiveSpaceAsFullScreenAuxiliary() {
        let normalized = HelmSettingsWindowSpacePolicy.normalizedCollectionBehavior([])

        XCTAssertTrue(normalized.contains(.moveToActiveSpace))
        XCTAssertTrue(normalized.contains(.auxiliary))
        XCTAssertTrue(normalized.contains(.fullScreenAuxiliary))
        XCTAssertFalse(normalized.contains(.canJoinAllSpaces))
    }

    func testSettingsWindowPolicyRemovesConflictingBehaviorAndPreservesCompatibleFlags() {
        let normalized = HelmSettingsWindowSpacePolicy.normalizedCollectionBehavior([
            .fullScreenPrimary,
            .fullScreenNone,
            .transient
        ])

        XCTAssertFalse(normalized.contains(.fullScreenPrimary))
        XCTAssertFalse(normalized.contains(.fullScreenNone))
        XCTAssertTrue(normalized.contains(.transient))
        XCTAssertTrue(normalized.contains(.moveToActiveSpace))
        XCTAssertTrue(normalized.contains(.auxiliary))
        XCTAssertTrue(normalized.contains(.fullScreenAuxiliary))
        XCTAssertFalse(normalized.contains(.canJoinAllSpaces))
    }

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

    func testRequestOpenDefersUntilModernBridgeRegistersWithoutVenturaFallback() throws {
        guard #available(macOS 14.0, *) else {
            throw XCTSkip("Modern Settings bridge behavior requires macOS 14 or newer")
        }

        var activateCount = 0
        var venturaOpenCount = 0
        let router = HelmSettingsOpenRouter(
            activateApp: { activateCount += 1 },
            openVenturaSettings: { venturaOpenCount += 1 }
        )
        var deferredOpenCount = 0
        let openExpectation = expectation(description: "Deferred Settings request opens")

        router.requestOpen()

        XCTAssertEqual(activateCount, 1)
        XCTAssertEqual(venturaOpenCount, 0)

        let registrationID = router.register {
            deferredOpenCount += 1
            openExpectation.fulfill()
        }
        wait(for: [openExpectation], timeout: 1)

        XCTAssertEqual(deferredOpenCount, 1)
        XCTAssertEqual(venturaOpenCount, 0)
        router.unregister(registrationID)
    }

    func testRequestOpenUsesRegisteredModernBridgeWithoutVenturaFallback() throws {
        guard #available(macOS 14.0, *) else {
            throw XCTSkip("Modern Settings bridge behavior requires macOS 14 or newer")
        }

        var activateCount = 0
        var venturaOpenCount = 0
        var registeredOpenCount = 0
        let router = HelmSettingsOpenRouter(
            activateApp: { activateCount += 1 },
            openVenturaSettings: { venturaOpenCount += 1 }
        )
        let registrationID = router.register {
            registeredOpenCount += 1
        }

        router.requestOpen()

        XCTAssertEqual(activateCount, 1)
        XCTAssertEqual(registeredOpenCount, 1)
        XCTAssertEqual(venturaOpenCount, 0)
        router.unregister(registrationID)
    }
}
