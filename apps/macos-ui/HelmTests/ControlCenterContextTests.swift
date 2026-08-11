import XCTest
@testable import Helm

final class ControlCenterContextTests: XCTestCase {
    func testPreviewDismissalStateIsSharedAcrossEntryPoints() {
        let context = ControlCenterContext()

        XCTAssertTrue(
            context.shouldPresentFirstRun(
                mode: .preview,
                hasCompletedOnboarding: true
            )
        )

        context.dismissedFirstRunPreview = true

        XCTAssertFalse(
            context.shouldPresentFirstRun(
                mode: .preview,
                hasCompletedOnboarding: true
            )
        )
    }

    func testPreviewDismissalDoesNotAffectEnabledFirstRunRoute() {
        let context = ControlCenterContext()
        context.dismissedFirstRunPreview = true

        XCTAssertTrue(
            context.shouldPresentFirstRun(
                mode: .enabled,
                hasCompletedOnboarding: false
            )
        )
        XCTAssertFalse(
            context.shouldPresentFirstRun(
                mode: .enabled,
                hasCompletedOnboarding: true
            )
        )
    }
}
