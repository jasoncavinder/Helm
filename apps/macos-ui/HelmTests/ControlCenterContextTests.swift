import XCTest

final class ControlCenterContextTests: XCTestCase {
    func testPreviewDismissalStateIsSharedAcrossPresentationChecks() {
        var session = EnvironmentBriefFirstRunSession()

        XCTAssertTrue(
            session.shouldPresent(
                mode: .preview,
                hasCompletedOnboarding: true
            )
        )

        session.dismissPreview()

        XCTAssertFalse(
            session.shouldPresent(
                mode: .preview,
                hasCompletedOnboarding: true
            )
        )
    }

    func testPreviewDismissalDoesNotAffectEnabledFirstRunRoute() {
        var session = EnvironmentBriefFirstRunSession()
        session.dismissPreview()

        XCTAssertTrue(
            session.shouldPresent(
                mode: .enabled,
                hasCompletedOnboarding: false
            )
        )
        XCTAssertFalse(
            session.shouldPresent(
                mode: .enabled,
                hasCompletedOnboarding: true
            )
        )
    }
}
