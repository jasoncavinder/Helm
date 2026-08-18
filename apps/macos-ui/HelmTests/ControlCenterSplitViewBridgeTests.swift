import SwiftUI
import XCTest

final class ControlCenterSplitViewBridgeTests: XCTestCase {
    func testNativeSidebarVisibilityPolicyMapsBooleanState() {
        XCTAssertEqual(
            NativeSidebarVisibilityPolicy.splitViewVisibility(isSidebarVisible: true),
            .all
        )
        XCTAssertEqual(
            NativeSidebarVisibilityPolicy.splitViewVisibility(isSidebarVisible: false),
            .detailOnly
        )
    }

    func testNativeSidebarVisibilityPolicyTreatsOnlyDetailOnlyAsCollapsed() {
        XCTAssertFalse(
            NativeSidebarVisibilityPolicy.isSidebarVisible(for: .detailOnly)
        )
        XCTAssertTrue(
            NativeSidebarVisibilityPolicy.isSidebarVisible(for: .all)
        )
        XCTAssertTrue(
            NativeSidebarVisibilityPolicy.isSidebarVisible(for: .doubleColumn)
        )
        XCTAssertTrue(
            NativeSidebarVisibilityPolicy.isSidebarVisible(for: .automatic)
        )
    }
}
