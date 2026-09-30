import XCTest

final class ManagerPriorityReorderingTests: XCTestCase {
    private let order = ["alpha", "beta", "gamma", "delta"]

    private func session(_ manager: String = "beta") throws -> ManagerPriorityReorderSession {
        try XCTUnwrap(.init(managerID: manager, authorityKey: "standard", installedOrder: order))
    }

    func testMissingOrDuplicateManagerCannotStartDrag() {
        XCTAssertNil(ManagerPriorityReorderSession(managerID: "absent", authorityKey: "standard", installedOrder: order))
        XCTAssertNil(ManagerPriorityReorderSession(managerID: "alpha", authorityKey: "standard", installedOrder: ["alpha", "alpha"]))
        XCTAssertNil(ManagerPriorityReorderSession(managerID: "alpha", authorityKey: "standard", installedOrder: []))
    }

    func testPreviewMovesPlaceholderWithoutMutatingOriginalOrder() throws {
        var value = try session()
        XCTAssertTrue(value.propose(targetID: "delta", after: true, authorityKey: "standard"))
        XCTAssertEqual(value.proposedOrder, ["alpha", "gamma", "delta", "beta"])
        XCTAssertEqual(value.originalOrder, order)
        XCTAssertEqual(value.validatedOrder(currentInstalledOrder: order, authorityKey: "standard"), value.proposedOrder)
    }

    func testUpwardDownwardBeforeAfterAndEndpoints() throws {
        var value = try session()
        for (target, after, expected) in [
            ("alpha", false, ["beta", "alpha", "gamma", "delta"]),
            ("alpha", true, order),
            ("gamma", false, order),
            ("gamma", true, ["alpha", "gamma", "beta", "delta"]),
            ("delta", false, ["alpha", "gamma", "beta", "delta"]),
            ("delta", true, ["alpha", "gamma", "delta", "beta"])
        ] {
            XCTAssertTrue(value.propose(targetID: target, after: after, authorityKey: "standard"))
            XCTAssertEqual(value.proposedOrder, expected)
        }
    }

    func testInvalidTargetsAndAuthoritiesDoNotChangePreview() throws {
        var value = try session()
        XCTAssertTrue(value.propose(targetID: "delta", after: true, authorityKey: "standard"))
        let expected = value.proposedOrder
        XCTAssertFalse(value.propose(targetID: "absent", after: false, authorityKey: "standard"))
        XCTAssertFalse(value.propose(targetID: "alpha", after: false, authorityKey: "guarded"))
        XCTAssertFalse(value.propose(targetID: "beta", after: false, authorityKey: "standard"))
        XCTAssertEqual(value.proposedOrder, expected)
    }

    func testStaleOrderNewDetectionAndRemovedInstallationRejectCommit() throws {
        let value = try session()
        XCTAssertNil(value.validatedOrder(currentInstalledOrder: Array(order.reversed()), authorityKey: "standard"))
        XCTAssertNil(value.validatedOrder(currentInstalledOrder: order + ["new"], authorityKey: "standard"))
        XCTAssertNil(value.validatedOrder(currentInstalledOrder: order.filter { $0 != "beta" }, authorityKey: "standard"))
        XCTAssertNil(value.validatedOrder(currentInstalledOrder: order, authorityKey: "guarded"))
    }

    func testFilteredTargetsPreserveRelativeOrderOfHiddenManagers() throws {
        var value = try session("alpha")
        XCTAssertTrue(value.propose(targetID: "delta", after: true, authorityKey: "standard"))
        XCTAssertEqual(value.proposedOrder, ["beta", "gamma", "delta", "alpha"])
        XCTAssertEqual(value.proposedOrder.filter { $0 != "alpha" }, order.filter { $0 != "alpha" })
    }

    func testRepeatedHoverIsIdempotentAndCanReturnToOriginalSlot() throws {
        var value = try session()
        for _ in 0..<100 { XCTAssertTrue(value.propose(targetID: "delta", after: true, authorityKey: "standard")) }
        XCTAssertEqual(value.proposedOrder, ["alpha", "gamma", "delta", "beta"])
        XCTAssertTrue(value.propose(targetID: "alpha", after: true, authorityKey: "standard"))
        XCTAssertEqual(value.proposedOrder, order)
    }

    func testEverySourceTargetCombinationPreservesMembershipAndOtherOrder() throws {
        let longOrder = (0..<100).map { "manager-\($0)" }
        for manager in longOrder {
            var value = try XCTUnwrap(ManagerPriorityReorderSession(managerID: manager, authorityKey: "standard", installedOrder: longOrder))
            for target in longOrder where target != manager {
                for after in [false, true] {
                    XCTAssertTrue(value.propose(targetID: target, after: after, authorityKey: "standard"))
                    XCTAssertNotNil(value.validatedOrder(currentInstalledOrder: longOrder, authorityKey: "standard"))
                }
            }
        }
    }

    func testCancellationRestoresUnmodifiedStateAndReleasesNativeSource() throws {
        let state = ManagerPriorityDragState()
        let token = try XCTUnwrap(state.begin(managerID: "beta", authorityKey: "standard", installedOrder: order, rowHeight: 140))
        state.nativeSource = NSObject()
        state.propose(targetID: "delta", after: true, authorityKey: "standard")
        XCTAssertEqual(state.rowHeight, 140)
        state.cancel(id: token)
        XCTAssertNil(state.session)
        XCTAssertNil(state.nativeSource)
        XCTAssertEqual(order, ["alpha", "beta", "gamma", "delta"])
    }

    func testStaleNativeCompletionCannotCancelNewSession() throws {
        let state = ManagerPriorityDragState()
        let old = try XCTUnwrap(state.begin(managerID: "beta", authorityKey: "standard", installedOrder: order, rowHeight: 100))
        XCTAssertNil(state.begin(managerID: "alpha", authorityKey: "standard", installedOrder: order, rowHeight: 100))
        state.cancel()
        let next = try XCTUnwrap(state.begin(managerID: "alpha", authorityKey: "standard", installedOrder: order, rowHeight: 100))
        state.cancel(id: old)
        XCTAssertEqual(state.session?.id, next)
    }

    func testInvalidGeometryCannotCreateInvalidPlaceholder() {
        for height in [CGFloat.nan, CGFloat.infinity, -10, 0] {
            let state = ManagerPriorityDragState()
            XCTAssertNotNil(state.begin(managerID: "beta", authorityKey: "standard", installedOrder: order, rowHeight: height))
            XCTAssertTrue(state.rowHeight.isFinite)
            XCTAssertGreaterThanOrEqual(state.rowHeight, 60)
        }
    }
}
