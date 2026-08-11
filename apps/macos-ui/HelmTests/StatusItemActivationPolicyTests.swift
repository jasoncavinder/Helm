import XCTest

final class StatusItemActivationPolicyTests: XCTestCase {
    func testEitherClickRoutesToPopoverWhileDashboardIsClosed() {
        XCTAssertEqual(
            route(clickKind: .primary),
            .popover
        )
        XCTAssertEqual(
            route(clickKind: .secondary),
            .popover
        )
    }

    func testPrimaryClickRoutesToDashboardWhileDashboardIsOpen() {
        XCTAssertEqual(
            route(clickKind: .primary, isDashboardVisible: true),
            .dashboard
        )
    }

    func testSecondaryClickRoutesToPopoverWhileDashboardIsOpen() {
        XCTAssertEqual(
            route(clickKind: .secondary, isDashboardVisible: true),
            .popover
        )
    }

    private func route(
        clickKind: StatusItemClickKind,
        isDashboardVisible: Bool = false
    ) -> StatusItemActivationRoute {
        StatusItemActivationPolicy.route(
            clickKind: clickKind,
            isDashboardVisible: isDashboardVisible
        )
    }
}
