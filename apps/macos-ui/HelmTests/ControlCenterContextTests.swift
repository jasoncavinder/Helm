import XCTest

final class ControlCenterContextTests: XCTestCase {
    func testReviewedPlanConfirmationPreservesTheReviewedSelection() {
        var presentation = UpgradeSheetPresentationState()
        let selectedStepIDs: Set<String> = ["update-mise-node", "update-mas-pages"]

        presentation.presentReviewedPlan(
            in: .controlCenter,
            managerScopeID: "mise",
            packageFilter: "node",
            selectedStepIDs: selectedStepIDs
        )

        XCTAssertTrue(presentation.isPresented)
        let request: ReviewedUpgradePlanRequest
        if case let .reviewedPlan(value) = presentation.intent {
            request = value
        } else {
            XCTFail("Expected a reviewed Plan confirmation request")
            return
        }
        XCTAssertEqual(request.managerScopeID, "mise")
        XCTAssertEqual(request.packageFilter, "node")
        XCTAssertEqual(request.selectedStepIDs, selectedStepIDs)

        presentation.dismiss()
        XCTAssertFalse(presentation.isPresented)

        presentation.presentUpgradeAll(in: .controlCenter)
        XCTAssertTrue(presentation.isPresented)
        XCTAssertEqual(presentation.intent, .upgradeAll)
    }

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
