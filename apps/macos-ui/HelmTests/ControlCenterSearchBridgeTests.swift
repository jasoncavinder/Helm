import XCTest

final class ControlCenterSearchFocusRouterTests: XCTestCase {
    func testRequestBeforeAttachmentIsDeliveredWhenTargetAttaches() {
        let router = ControlCenterSearchFocusRouter()
        let target = SearchFocusTargetSpy()

        router.requestFocus()
        XCTAssertEqual(target.requestCount, 0)

        router.attach(target)
        XCTAssertEqual(target.requestCount, 1)

        target.completeFocusRequest()
        router.detach(target)
        router.attach(target)
        XCTAssertEqual(target.requestCount, 1)
    }

    func testUnfulfilledRequestMovesToReplacementTarget() {
        let router = ControlCenterSearchFocusRouter()
        let firstTarget = SearchFocusTargetSpy()
        let replacementTarget = SearchFocusTargetSpy()

        router.attach(firstTarget)
        router.requestFocus()
        router.detach(firstTarget)
        router.attach(replacementTarget)

        XCTAssertEqual(firstTarget.requestCount, 1)
        XCTAssertEqual(replacementTarget.requestCount, 1)
    }
}

final class ControlCenterSearchTextUpdateGateTests: XCTestCase {
    func testModelUpdateCannotPublishBackThroughControlDelegate() {
        let gate = ControlCenterSearchTextUpdateGate()
        var shouldPublishDuringUpdate = true

        gate.applyModelValue {
            shouldPublishDuringUpdate = gate.shouldPublishControlValue(
                "model value",
                modelValue: "old value"
            )
        }

        XCTAssertFalse(shouldPublishDuringUpdate)
        XCTAssertTrue(
            gate.shouldPublishControlValue("user value", modelValue: "model value")
        )
        XCTAssertFalse(
            gate.shouldPublishControlValue("model value", modelValue: "model value")
        )
    }

    func testControlUpdatesCoalesceWithoutBeingOverwrittenByStaleModel() {
        let gate = ControlCenterSearchTextUpdateGate()

        XCTAssertTrue(gate.stageControlValue("h", modelValue: ""))
        XCTAssertFalse(gate.stageControlValue("he", modelValue: ""))
        XCTAssertEqual(gate.displayedValue(modelValue: ""), "he")
        XCTAssertEqual(gate.takePendingControlValue(), "he")
        XCTAssertFalse(gate.hasScheduledControlPublish)
        XCTAssertEqual(gate.displayedValue(modelValue: "he"), "he")
    }
}

private final class SearchFocusTargetSpy: ControlCenterSearchFocusTarget {
    private(set) var requestCount = 0
    private var completion: (() -> Void)?

    func requestSearchFocus(completion: @escaping () -> Void) {
        requestCount += 1
        self.completion = completion
    }

    func completeFocusRequest() {
        completion?()
        completion = nil
    }
}
