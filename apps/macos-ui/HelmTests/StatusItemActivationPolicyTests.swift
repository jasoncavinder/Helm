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

    func testEitherClickRoutesToDashboardWhenFirstRunShouldPresent() {
        XCTAssertEqual(
            route(clickKind: .primary, shouldPresentFirstRun: true),
            .dashboard
        )
        XCTAssertEqual(
            route(clickKind: .secondary, shouldPresentFirstRun: true),
            .dashboard
        )
    }

    private func route(
        clickKind: StatusItemClickKind,
        isDashboardVisible: Bool = false,
        shouldPresentFirstRun: Bool = false
    ) -> StatusItemActivationRoute {
        StatusItemActivationPolicy.route(
            clickKind: clickKind,
            isDashboardVisible: isDashboardVisible,
            shouldPresentFirstRun: shouldPresentFirstRun
        )
    }
}
