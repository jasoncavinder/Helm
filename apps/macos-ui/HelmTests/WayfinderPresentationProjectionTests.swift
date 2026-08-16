import XCTest

final class WayfinderPresentationProjectionTests: XCTestCase {
    func testPriorityOrderSelectsOnlyHighestCondition() {
        let input = WayfinderProjectionInput(
            serviceAvailable: false,
            approvalTaskIDs: ["approval"],
            failedTaskIDs: ["failed"],
            interruptedTaskIDs: ["interrupted"],
            activeTaskIDs: ["active"],
            actionableFindingIDs: ["manager"],
            updateCount: 4,
            isRefreshing: true
        )

        let projection = project(input)

        XCTAssertEqual(projection.content.mode, .approval)
        XCTAssertEqual(projection.content.condition, .approvalRequired(count: 1))
        XCTAssertEqual(projection.content.primaryAction.destination, .activity)
        XCTAssertEqual(projection.content.primaryAction.entityID, "approval")
    }

    func testFailedAndInterruptedWorkPrecedesActiveWork() {
        let input = WayfinderProjectionInput(
            failedTaskIDs: ["failed"],
            interruptedTaskIDs: ["interrupted"],
            activeTaskIDs: ["active"]
        )

        let projection = project(input)

        XCTAssertEqual(projection.content.mode, .failedInterrupted)
        XCTAssertEqual(
            projection.content.condition,
            .failedOrInterrupted(failed: 1, interrupted: 1)
        )
        XCTAssertEqual(projection.content.primaryAction.entityID, "failed")
    }

    func testActiveWorkUsesOnlyValidatedBackendProgress() throws {
        let validProgress = try XCTUnwrap(
            WayfinderDeterminateProgress(completed: 2, total: 5)
        )
        let determinate = project(
            WayfinderProjectionInput(
                activeTaskIDs: ["active"],
                activeProgress: validProgress,
                currentAuthorityStage: "standard"
            )
        )
        let indeterminate = project(
            WayfinderProjectionInput(activeTaskIDs: ["active"])
        )

        XCTAssertEqual(determinate.content.mode, .determinateWork)
        XCTAssertEqual(
            try XCTUnwrap(determinate.content.progress).fraction,
            0.4,
            accuracy: 0.0001
        )
        XCTAssertEqual(determinate.content.currentAuthorityStage, "standard")
        XCTAssertEqual(indeterminate.content.mode, .indeterminateWork)
        XCTAssertNil(indeterminate.content.progress)
        XCTAssertNil(WayfinderDeterminateProgress(completed: 6, total: 5))
        XCTAssertNil(WayfinderDeterminateProgress(completed: -1, total: 5))
        XCTAssertNil(WayfinderDeterminateProgress(completed: 0, total: 0))
    }

    func testActionableFindingRoutesToEnvironmentBeforeUpdates() {
        let projection = project(
            WayfinderProjectionInput(
                actionableFindingIDs: ["npm"],
                updateCount: 3
            )
        )

        XCTAssertEqual(projection.content.mode, .cachedPartialOffline)
        XCTAssertEqual(projection.content.condition, .actionableFinding(count: 1))
        XCTAssertEqual(projection.content.primaryAction.destination, .environment)
        XCTAssertEqual(projection.content.primaryAction.entityID, "npm")
    }

    func testUpdatesUseCountNotPercentage() {
        let projection = project(WayfinderProjectionInput(updateCount: 7))

        XCTAssertEqual(projection.content.mode, .updatesReady)
        XCTAssertEqual(projection.content.condition, .updatesReady(count: 7))
        XCTAssertEqual(projection.content.title.arguments, ["count": "7"])
        XCTAssertNil(projection.content.progress)
        XCTAssertEqual(projection.content.primaryAction.destination, .plan)
    }

    func testRefreshAndUnavailableRemainDistinctConditions() {
        let refreshing = project(
            WayfinderProjectionInput(serviceAvailable: false, isRefreshing: true)
        )
        let unavailable = project(
            WayfinderProjectionInput(serviceAvailable: false)
        )

        XCTAssertEqual(refreshing.content.mode, .indeterminateWork)
        XCTAssertEqual(refreshing.content.condition, .refreshing)
        XCTAssertEqual(unavailable.content.mode, .cachedPartialOffline)
        XCTAssertEqual(unavailable.content.condition, .serviceUnavailable)
        XCTAssertEqual(unavailable.content.primaryAction.destination, .dashboard)
        XCTAssertEqual(unavailable.content.primaryAction.focus, .serviceHealth)
        XCTAssertEqual(unavailable.content.primaryActionTitle.key, "app.inspector.view_diagnostics")
    }

    func testOfflineIsDistinctFromServiceUnavailableAndPrecedesCachedUpdates() {
        let offline = project(
            WayfinderProjectionInput(networkAvailable: false, updateCount: 3)
        )
        let unavailable = project(
            WayfinderProjectionInput(serviceAvailable: false)
        )

        XCTAssertEqual(offline.content.mode, .cachedPartialOffline)
        XCTAssertEqual(offline.content.condition, .offline)
        XCTAssertEqual(offline.content.primaryAction.destination, .dashboard)
        XCTAssertEqual(offline.content.primaryAction.focus, .serviceHealth)
        XCTAssertEqual(
            offline.content.title.key,
            "app.wayfinder.course.offline.title"
        )
        XCTAssertEqual(unavailable.content.condition, .serviceUnavailable)
    }

    func testHealthyProjectionRoutesToDashboard() {
        let projection = project(.healthy)

        XCTAssertEqual(projection.content.mode, .healthy)
        XCTAssertEqual(projection.content.condition, .healthy)
        XCTAssertEqual(projection.content.primaryAction.destination, .dashboard)
        XCTAssertEqual(projection.content.primaryAction.focus, .primaryContent)
    }

    func testFooterStatusesKeepUpdatesDistinctFromReviewFindings() {
        let updates = project(WayfinderProjectionInput(updateCount: 2)).content
        let finding = project(
            WayfinderProjectionInput(actionableFindingIDs: ["npm"])
        ).content
        let running = project(WayfinderProjectionInput(activeTaskIDs: ["task-1"])).content

        XCTAssertEqual(updates.condition.footerStatus, .updatesReady)
        XCTAssertNil(updates.condition.sidebarFooterStatus)
        XCTAssertEqual(updates.primaryAction.destination, .plan)
        XCTAssertEqual(finding.condition.footerStatus, .needsReview)
        XCTAssertEqual(finding.condition.sidebarFooterStatus, .needsReview)
        XCTAssertEqual(finding.primaryAction.destination, .environment)
        XCTAssertEqual(finding.primaryAction.entityID, "npm")
        XCTAssertEqual(running.condition.footerStatus, .running)
        XCTAssertEqual(running.primaryAction.destination, .activity)
        XCTAssertEqual(running.primaryAction.entityID, "task-1")
        XCTAssertFalse(WayfinderCondition.healthy.footerStatus.isActionable)
        XCTAssertEqual(WayfinderCondition.healthy.sidebarFooterStatus, .healthy)
    }

    func testOfflineAndUnavailableUseNeutralFooterStatus() {
        XCTAssertEqual(WayfinderCondition.offline.footerStatus, .unavailable)
        XCTAssertEqual(WayfinderCondition.serviceUnavailable.footerStatus, .unavailable)
        XCTAssertEqual(WayfinderCondition.offline.sidebarFooterStatus, .unavailable)
        XCTAssertEqual(WayfinderCondition.serviceUnavailable.sidebarFooterStatus, .unavailable)
    }

    func testDashboardManagerCountsIncludeDisabledAndUndetectedManagers() {
        let counts = DashboardManagerCounts(statuses: [
            (detected: true, enabled: true, isImplemented: true),
            (detected: true, enabled: false, isImplemented: true),
            (detected: false, enabled: true, isImplemented: true),
            (detected: false, enabled: false, isImplemented: true),
            (detected: true, enabled: true, isImplemented: false),
        ])

        XCTAssertEqual(counts.detected, 2)
        XCTAssertEqual(counts.disabled, 1)
        XCTAssertEqual(counts.available, 2)
    }

    func testRevisionChangesOnlyWhenSemanticContentChanges() {
        let first = WayfinderProjectionProjector.project(
            WayfinderProjectionInput(updateCount: 2),
            replacing: .initial
        )
        let unchanged = WayfinderProjectionProjector.project(
            WayfinderProjectionInput(updateCount: 2),
            replacing: first
        )
        let changed = WayfinderProjectionProjector.project(
            WayfinderProjectionInput(updateCount: 3),
            replacing: unchanged
        )

        XCTAssertEqual(first.revision, 1)
        XCTAssertEqual(unchanged.revision, first.revision)
        XCTAssertEqual(changed.revision, first.revision + 1)
    }

    private func project(
        _ input: WayfinderProjectionInput
    ) -> WayfinderPresentationProjection {
        WayfinderProjectionProjector.project(input, replacing: .initial)
    }
}
