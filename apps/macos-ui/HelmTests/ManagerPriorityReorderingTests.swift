import XCTest

final class ManagerPriorityReorderingTests: XCTestCase {
    private let order = ["alpha", "beta", "gamma", "delta"]

    private var frames: [String: CGRect] {
        Dictionary(uniqueKeysWithValues: order.enumerated().map { index, id in
            (id, CGRect(x: 0, y: index * 108, width: 500, height: 100))
        })
    }

    private func drag(_ state: ManagerPriorityDragState, to y: CGFloat,
                      frames: [String: CGRect]? = nil, x: CGFloat = 20) throws -> Bool {
        let session = try XCTUnwrap(state.session)
        return state.proposeOverlap(dragFrame: CGRect(x: x, y: y - 22, width: 260, height: 44),
                                    pointerY: y, targetFrames: frames ?? self.frames,
                                    authorityKey: "standard", token: session.id)
    }

    private func activeDrag() throws -> ManagerPriorityDragState {
        let state = ManagerPriorityDragState()
        XCTAssertNotNil(state.begin(managerID: "beta", authorityKey: "standard", installedOrder: order, rowHeight: 100))
        XCTAssertFalse(try drag(state, to: 150))
        return state
    }

    func testDownwardPreviewMovesAtFirstTopEdgeOverlapBeforeCursorReachesCard() throws {
        let state = try activeDrag()
        XCTAssertFalse(try drag(state, to: 193)) // Image bottom 215, next card starts at 216.
        XCTAssertEqual(state.session?.proposedOrder, order)
        XCTAssertTrue(try drag(state, to: 194))
        XCTAssertEqual(state.session?.proposedOrder, ["alpha", "gamma", "beta", "delta"])
    }

    func testUpwardPreviewMovesAtFirstBottomEdgeOverlapBeforeCursorReachesCard() throws {
        let state = try activeDrag()
        XCTAssertFalse(try drag(state, to: 123))
        XCTAssertTrue(try drag(state, to: 122)) // Image top 100, previous card ends at 100.
        XCTAssertEqual(state.session?.proposedOrder, ["beta", "alpha", "gamma", "delta"])
    }

    func testReflowStationaryPointerAndSmallJitterDoNotUndoButDeliberateReversalDoes() throws {
        let state = try activeDrag()
        XCTAssertTrue(try drag(state, to: 194))
        var reflowed = frames
        reflowed["gamma"] = frames["beta"]
        reflowed["beta"] = frames["gamma"]
        for _ in 0..<20 { XCTAssertFalse(try drag(state, to: 194, frames: reflowed)) }
        XCTAssertFalse(try drag(state, to: 195, frames: reflowed))
        XCTAssertFalse(try drag(state, to: 193, frames: reflowed))
        XCTAssertEqual(state.session?.proposedOrder, ["alpha", "gamma", "beta", "delta"])
        XCTAssertTrue(try drag(state, to: 191, frames: reflowed))
        XCTAssertEqual(state.session?.proposedOrder, order)
    }

    func testFastMovementAndFilteredGeometryPreserveOtherManagers() throws {
        let state = try activeDrag()
        XCTAssertTrue(try drag(state, to: 400, frames: ["delta": try XCTUnwrap(frames["delta"])]))
        XCTAssertEqual(state.session?.proposedOrder, ["alpha", "gamma", "delta", "beta"])
        XCTAssertTrue(try drag(state, to: 60, frames: ["alpha": try XCTUnwrap(frames["alpha"])]))
        XCTAssertEqual(state.session?.proposedOrder, ["beta", "alpha", "gamma", "delta"])
    }

    func testHorizontalMissMissingManagerAndInvalidGeometryDoNotMovePreview() throws {
        let state = try activeDrag()
        XCTAssertFalse(try drag(state, to: 194, x: 501))
        XCTAssertFalse(try drag(state, to: 200, frames: ["not-installed": CGRect(x: 0, y: 108, width: 500, height: 100)]))
        XCTAssertFalse(try drag(state, to: 210, frames: ["gamma": CGRect(x: 0, y: 216, width: CGFloat.infinity, height: 100)]))
        XCTAssertEqual(state.session?.proposedOrder, order)
    }

    func testOverlapRejectsForeignAuthorityAndStaleSessionWithoutConsumingMovement() throws {
        let state = try activeDrag()
        let token = try XCTUnwrap(state.session?.id)
        let frame = CGRect(x: 20, y: 172, width: 260, height: 44)
        XCTAssertFalse(state.proposeOverlap(dragFrame: frame, pointerY: 194, targetFrames: frames,
                                           authorityKey: "guarded", token: token))
        XCTAssertFalse(state.proposeOverlap(dragFrame: frame, pointerY: 194, targetFrames: frames,
                                           authorityKey: "standard", token: UUID()))
        XCTAssertFalse(state.proposeOverlap(dragFrame: frame, pointerY: .nan, targetFrames: frames,
                                           authorityKey: "standard", token: token))
        XCTAssertTrue(try drag(state, to: 194))
    }

    func testCancellationResetsOverlapMotionForNextDrag() throws {
        let state = try activeDrag()
        XCTAssertTrue(try drag(state, to: 400))
        state.cancel()
        XCTAssertNotNil(state.begin(managerID: "beta", authorityKey: "standard", installedOrder: order, rowHeight: 100))
        XCTAssertFalse(try drag(state, to: 150))
        XCTAssertTrue(try drag(state, to: 122))
    }

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
