import XCTest

final class ControlCenterContextTests: XCTestCase {
    func testReviewedPlanConfirmationPreservesTheReviewedSelection() {
        var presentation = UpgradeSheetPresentationState()
        let selectedSteps = [
            planStep(id: "update-mise-node", orderIndex: 1, managerID: "mise"),
            planStep(id: "update-mas-pages", orderIndex: 2, managerID: "mas"),
        ]
        let automaticStepIDs: Set<String> = ["update-mise-node", "update-mas-pages"]
        let riskSummary = UpgradePreviewPlanner.RiskSummary(
            requiresElevatedPrivileges: true,
            mayRequireReboot: false
        )

        presentation.presentReviewedPlan(
            in: .controlCenter,
            managerScopeID: "mise",
            packageFilter: "node",
            selectedSteps: selectedSteps,
            automaticallyRunStepIDs: automaticStepIDs,
            riskSummary: riskSummary
        )

        XCTAssertTrue(presentation.isPresented)
        guard let request = presentation.reviewedPlanRequest else {
            XCTFail("Expected a reviewed Plan confirmation request")
            return
        }
        XCTAssertEqual(request.managerScopeID, "mise")
        XCTAssertEqual(request.packageFilter, "node")
        XCTAssertEqual(request.selectedSteps, selectedSteps)
        XCTAssertEqual(request.selectedStepIDs, Set(selectedSteps.map(\.id)))
        XCTAssertEqual(request.automaticallyRunStepIDs, automaticStepIDs)
        XCTAssertEqual(request.riskSummary, riskSummary)

        presentation.dismiss()
        XCTAssertFalse(presentation.isPresented)
        XCTAssertEqual(presentation.reviewedPlanRequest, request)
    }

    func testConfirmationRequestAdvancesToken() {
        var state = UpgradePlanConfirmationRequestState()

        state.requestUpgradeAll()

        XCTAssertEqual(state.token, 1)

        state.requestUpgradeAll()
        XCTAssertEqual(state.token, 2)
    }

    func testReviewedPlanValidationAcceptsAnUnchangedSnapshot() {
        let request = reviewedRequest()

        XCTAssertTrue(
            ReviewedUpgradePlanValidation.isCurrent(
                request: request,
                currentSelectedSteps: request.selectedSteps,
                currentAutomaticallyRunStepIDs: request.automaticallyRunStepIDs,
                currentRiskSummary: request.riskSummary
            )
        )
    }

    func testReviewedPlanValidationRejectsChangedStepsEligibilityAndRisk() {
        let request = reviewedRequest()
        var reorderedSteps = request.selectedSteps
        reorderedSteps.swapAt(0, 1)

        XCTAssertFalse(
            ReviewedUpgradePlanValidation.isCurrent(
                request: request,
                currentSelectedSteps: reorderedSteps,
                currentAutomaticallyRunStepIDs: request.automaticallyRunStepIDs,
                currentRiskSummary: request.riskSummary
            )
        )
        XCTAssertFalse(
            ReviewedUpgradePlanValidation.isCurrent(
                request: request,
                currentSelectedSteps: request.selectedSteps,
                currentAutomaticallyRunStepIDs: ["update-mise-node"],
                currentRiskSummary: request.riskSummary
            )
        )
        XCTAssertFalse(
            ReviewedUpgradePlanValidation.isCurrent(
                request: request,
                currentSelectedSteps: request.selectedSteps,
                currentAutomaticallyRunStepIDs: request.automaticallyRunStepIDs,
                currentRiskSummary: .init(
                    requiresElevatedPrivileges: false,
                    mayRequireReboot: true
                )
            )
        )
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

    private func planStep(
        id: String,
        orderIndex: UInt64,
        managerID: String
    ) -> ReviewedUpgradePlanStep {
        ReviewedUpgradePlanStep(
            id: id,
            orderIndex: orderIndex,
            managerID: managerID,
            authority: "standard",
            action: "upgrade",
            packageName: id,
            reasonLabelKey: "service.task.label.upgrade.package",
            reasonLabelArgs: ["package": id],
            status: "queued"
        )
    }

    private func reviewedRequest() -> ReviewedUpgradePlanRequest {
        ReviewedUpgradePlanRequest(
            managerScopeID: "",
            packageFilter: "",
            selectedSteps: [
                planStep(id: "update-mise-node", orderIndex: 1, managerID: "mise"),
                planStep(id: "update-mas-pages", orderIndex: 2, managerID: "mas"),
            ],
            automaticallyRunStepIDs: ["update-mise-node", "update-mas-pages"],
            riskSummary: .init(
                requiresElevatedPrivileges: true,
                mayRequireReboot: false
            )
        )
    }
}
