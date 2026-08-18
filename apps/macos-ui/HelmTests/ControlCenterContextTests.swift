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

    func testConfirmationRequestIsConsumedOnlyOnceAcrossPlanRemounts() {
        var state = UpgradePlanConfirmationRequestState()
        state.requestUpgradeAll()

        guard let request = state.pendingRequest else {
            XCTFail("Expected a pending confirmation request")
            return
        }
        var previewState = UpgradePlanPreviewRevisionState()
        let previewRequest = previewState.issue(includePinned: false, allowOsUpdates: false)
        XCTAssertTrue(state.requirePreview(previewRequest, for: request))
        guard let boundRequest = state.pendingRequest else {
            XCTFail("Expected a preview-bound confirmation request")
            return
        }
        XCTAssertEqual(boundRequest.requiredPreviewRevision, previewRequest.revision)
        XCTAssertTrue(state.complete(boundRequest, presentationSucceeded: true))
        XCTAssertNil(state.pendingRequest)
        XCTAssertEqual(state.lastConsumedRequestID, boundRequest.id)

        // A newly mounted Plan reads the same persistent state and finds no request to replay.
        XCTAssertNil(state.pendingRequest)
        XCTAssertFalse(state.complete(boundRequest, presentationSucceeded: true))
    }

    func testNotificationConfirmationRequestEnforcesNoPinnedOrOsUpdates() {
        var state = UpgradePlanConfirmationRequestState()
        state.requestUpgradeAll()

        guard let request = state.pendingRequest else {
            XCTFail("Expected a pending confirmation request")
            return
        }
        XCTAssertFalse(request.includePinned)
        XCTAssertFalse(request.allowOsUpdates)
        XCTAssertTrue(request.includes(managerID: "brew", isPinned: false))
        XCTAssertFalse(request.includes(managerID: "brew", isPinned: true))
        XCTAssertFalse(request.includes(managerID: "softwareupdate", isPinned: false))
    }

    func testConfirmationRequestRemainsPendingForEmptyOrUnavailablePresentation() {
        var state = UpgradePlanConfirmationRequestState()
        state.requestUpgradeAll()

        guard let request = state.pendingRequest else {
            XCTFail("Expected a pending confirmation request")
            return
        }
        let emptyPlanCanPresent = ReviewedUpgradePlanPresentationPolicy.canPresent(
            automaticallyRunStepIDs: [],
            executionAvailable: true
        )
        XCTAssertFalse(emptyPlanCanPresent)
        XCTAssertFalse(state.complete(request, presentationSucceeded: emptyPlanCanPresent))
        XCTAssertEqual(state.pendingRequest, request)

        let unavailablePlanCanPresent = ReviewedUpgradePlanPresentationPolicy.canPresent(
            automaticallyRunStepIDs: ["update-brew-ripgrep"],
            executionAvailable: false
        )
        XCTAssertFalse(unavailablePlanCanPresent)
        XCTAssertFalse(state.complete(request, presentationSucceeded: unavailablePlanCanPresent))
        XCTAssertEqual(state.pendingRequest, request)
        XCTAssertNil(state.lastConsumedRequestID)
    }

    func testUpgradePlanPreviewIgnoresOutOfOrderResponses() {
        var confirmationState = UpgradePlanConfirmationRequestState()
        confirmationState.requestUpgradeAll()
        guard let confirmationRequest = confirmationState.pendingRequest else {
            XCTFail("Expected a pending confirmation request")
            return
        }

        var previewState = UpgradePlanPreviewRevisionState()
        let first = previewState.issue(includePinned: false, allowOsUpdates: false)
        XCTAssertTrue(confirmationState.requirePreview(first, for: confirmationRequest))
        let second = previewState.issue(includePinned: false, allowOsUpdates: false)

        XCTAssertFalse(previewState.apply(first))
        XCTAssertNil(previewState.latestAppliedRequest)
        guard let firstBoundRequest = confirmationState.pendingRequest else {
            XCTFail("Expected a preview-bound confirmation request")
            return
        }
        XCTAssertFalse(
            UpgradePlanConfirmationPreviewPolicy.isReady(
                firstBoundRequest,
                previewState: previewState
            )
        )

        XCTAssertTrue(confirmationState.requirePreview(second, for: firstBoundRequest))
        XCTAssertTrue(previewState.apply(second))
        guard let secondBoundRequest = confirmationState.pendingRequest else {
            XCTFail("Expected a rebound confirmation request")
            return
        }
        XCTAssertTrue(
            UpgradePlanConfirmationPreviewPolicy.isReady(
                secondBoundRequest,
                previewState: previewState
            )
        )
    }

    func testUnappliedPreviewCanBeReboundAfterServiceRecovery() {
        var confirmationState = UpgradePlanConfirmationRequestState()
        confirmationState.requestUpgradeAll()
        guard let request = confirmationState.pendingRequest else {
            XCTFail("Expected a pending confirmation request")
            return
        }

        var previewState = UpgradePlanPreviewRevisionState()
        let unavailablePreview = previewState.issue(includePinned: false, allowOsUpdates: false)
        XCTAssertTrue(confirmationState.requirePreview(unavailablePreview, for: request))
        XCTAssertNil(previewState.latestAppliedRequest)

        let recoveredPreview = previewState.issue(includePinned: false, allowOsUpdates: false)
        guard let pendingRequest = confirmationState.pendingRequest else {
            XCTFail("Expected the request to remain pending")
            return
        }
        XCTAssertTrue(confirmationState.requirePreview(recoveredPreview, for: pendingRequest))
        XCTAssertTrue(previewState.apply(recoveredPreview))
        guard let reboundRequest = confirmationState.pendingRequest else {
            XCTFail("Expected the request to remain pending until presentation")
            return
        }
        XCTAssertEqual(reboundRequest.requiredPreviewRevision, recoveredPreview.revision)
        XCTAssertTrue(
            UpgradePlanConfirmationPreviewPolicy.isReady(
                reboundRequest,
                previewState: previewState
            )
        )
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
