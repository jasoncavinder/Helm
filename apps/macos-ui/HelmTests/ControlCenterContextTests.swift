import XCTest

final class ControlCenterContextTests: XCTestCase {
    func testPreviewDismissalStatePreventsPreviewRouteFromPresenting() {
        XCTAssertTrue(
            EnvironmentBriefFirstRunConfiguration.shouldPresent(
                mode: .preview,
                hasCompletedOnboarding: true,
                dismissedPreview: false
            )
        )

        XCTAssertFalse(
            EnvironmentBriefFirstRunConfiguration.shouldPresent(
                mode: .preview,
                hasCompletedOnboarding: true,
                dismissedPreview: true
            )
        )
    }

    func testPreviewDismissalDoesNotAffectEnabledFirstRunRoute() {
        XCTAssertTrue(
            EnvironmentBriefFirstRunConfiguration.shouldPresent(
                mode: .enabled,
                hasCompletedOnboarding: false,
                dismissedPreview: true
            )
        )
        XCTAssertFalse(
            EnvironmentBriefFirstRunConfiguration.shouldPresent(
                mode: .enabled,
                hasCompletedOnboarding: true,
                dismissedPreview: true
            )
        )
    }
}
