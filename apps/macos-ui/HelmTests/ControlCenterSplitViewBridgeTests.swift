import SwiftUI
import XCTest

final class ControlCenterSplitViewBridgeTests: XCTestCase {
    func testInspectorContentIsMountedOnlyWhilePresented() {
        var inspectorCreationCount = 0
        let controller = ControlCenterSplitViewController(
            content: AnyView(Text("Content")),
            inspector: {
                inspectorCreationCount += 1
                return AnyView(Text("Inspector"))
            }
        )

        _ = controller.view
        XCTAssertEqual(controller.splitViewItems.count, 2)
        XCTAssertTrue(controller.isInspectorCollapsed)
        XCTAssertFalse(controller.isInspectorContentMounted)
        XCTAssertEqual(inspectorCreationCount, 0)

        controller.setInspectorPresented(true)
        XCTAssertEqual(controller.splitViewItems.count, 2)
        XCTAssertFalse(controller.isInspectorCollapsed)
        XCTAssertTrue(controller.isInspectorContentMounted)
        XCTAssertEqual(inspectorCreationCount, 1)

        controller.setInspectorPresented(false)
        XCTAssertTrue(controller.isInspectorCollapsed)
        XCTAssertFalse(controller.isInspectorContentMounted)

        controller.setInspectorPresented(true)
        XCTAssertFalse(controller.isInspectorCollapsed)
        XCTAssertTrue(controller.isInspectorContentMounted)
        XCTAssertEqual(inspectorCreationCount, 2)
    }

    func testRepeatedPresentationUpdatesAreIdempotent() {
        var inspectorCreationCount = 0
        let controller = ControlCenterSplitViewController(
            content: AnyView(Text("Content")),
            inspector: {
                inspectorCreationCount += 1
                return AnyView(Text("Inspector"))
            }
        )

        controller.setInspectorPresented(true)
        controller.setInspectorPresented(true)
        XCTAssertEqual(inspectorCreationCount, 1)

        controller.setInspectorPresented(false)
        controller.setInspectorPresented(false)
        XCTAssertEqual(inspectorCreationCount, 1)
    }
}
