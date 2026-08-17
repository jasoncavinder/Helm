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
        XCTAssertEqual(updates.primaryAction.originatingCondition, .updatesReady)
        XCTAssertEqual(finding.condition.footerStatus, .needsReview)
        XCTAssertEqual(finding.condition.sidebarFooterStatus, .needsReview)
        XCTAssertEqual(finding.primaryAction.destination, .environment)
        XCTAssertEqual(finding.primaryAction.entityID, "npm")
        XCTAssertEqual(finding.primaryAction.originatingCondition, .actionableFinding)
        XCTAssertEqual(running.condition.footerStatus, .running)
        XCTAssertEqual(running.primaryAction.destination, .activity)
        XCTAssertEqual(running.primaryAction.entityID, "task-1")
        XCTAssertFalse(WayfinderCondition.healthy.footerStatus.isActionable)
        XCTAssertEqual(WayfinderCondition.healthy.sidebarFooterStatus, .healthy)
    }

    func testNavigationStatePreservesCompleteDeepLink() {
        let deepLink = WayfinderDeepLink(
            destination: .environment,
            entityID: "pnpm",
            focus: .selectedEntity,
            routeStage: .packages,
            originatingCondition: .actionableFinding
        )
        var navigationState = WayfinderNavigationState()

        navigationState.record(deepLink)

        XCTAssertEqual(navigationState.deepLink, deepLink)
    }

    func testEnvironmentRouteFilterAppliesScopedDeepLinkAndClearsForUnscopedNavigation() {
        let deepLink = WayfinderDeepLink(
            destination: .environment,
            entityID: nil,
            focus: .primaryContent,
            routeStage: .packages
        )
        var filterState = WayfinderEnvironmentRouteFilterState()

        filterState.apply(deepLink)
        XCTAssertEqual(filterState.stage, .packages)

        filterState.clear()
        XCTAssertNil(filterState.stage)
    }

    func testRouteItemDeepLinkPreservesDomainAndAffectedManager() {
        let routeItem = WayfinderPopoverRouteItem(
            stage: .packages,
            tone: .review,
            managerID: "pnpm"
        )

        XCTAssertEqual(
            routeItem.deepLink(originatingCondition: .actionableFinding),
            WayfinderDeepLink(
                destination: .environment,
                entityID: "pnpm",
                focus: .selectedEntity,
                routeStage: .packages,
                originatingCondition: .actionableFinding
            )
        )
    }

    func testRouteItemWithoutAffectedManagerStillPreservesDomain() {
        let routeItem = WayfinderPopoverRouteItem(stage: .system, tone: .current)

        XCTAssertEqual(
            routeItem.deepLink(originatingCondition: .healthy),
            WayfinderDeepLink(
                destination: .environment,
                entityID: nil,
                focus: .primaryContent,
                routeStage: .system,
                originatingCondition: .healthy
            )
        )
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

    func testPopoverRouteStagesMapManagerCategoriesWithoutTreatingRouteAsProgress() {
        XCTAssertEqual(WayfinderPopoverRouteStage.stage(forManagerCategory: "System/OS"), .system)
        XCTAssertEqual(WayfinderPopoverRouteStage.stage(forManagerCategory: "Toolchain"), .tools)
        XCTAssertEqual(WayfinderPopoverRouteStage.stage(forManagerCategory: "App Store"), .apps)
        XCTAssertEqual(WayfinderPopoverRouteStage.stage(forManagerCategory: "Language"), .packages)
    }

    func testHealthyPopoverHasReassuranceInsteadOfInventedAction() {
        let presentation = popover(
            projection: project(.healthy).content,
            detectedManagerCount: 7
        )

        XCTAssertFalse(presentation.showsPrimaryAction)
        XCTAssertTrue(presentation.allowsRefresh)
        XCTAssertEqual(
            presentation.routeItems.map(\.tone),
            Array(repeating: .current, count: 4)
        )
        XCTAssertEqual(
            presentation.contextDetail.arguments,
            ["count": "7"]
        )
    }

    func testActionableFindingPopoverNamesTheSpecificFindingContext() {
        let findingContext = WayfinderPopoverFindingContext(
            title: WayfinderLocalizedText(
                key: "app.popover.wayfinder.context.manager_needs_decision",
                arguments: ["manager": "pnpm"]
            ),
            detail: WayfinderLocalizedText(
                key: "app.inspector.multi_instance.attention_title"
            )
        )
        let presentation = popover(
            projection: project(
                WayfinderProjectionInput(actionableFindingIDs: ["pnpm"])
            ).content,
            relatedRouteStages: [.packages],
            relatedManagerIDsByStage: [.packages: "pnpm"],
            findingContext: findingContext
        )

        XCTAssertEqual(presentation.contextTitle, findingContext.title)
        XCTAssertEqual(presentation.contextDetail, findingContext.detail)
        XCTAssertEqual(
            presentation.routeItems.first(where: { $0.stage == .packages })?.tone,
            .review
        )
        XCTAssertEqual(
            presentation.routeItems.first(where: { $0.stage == .packages })?.managerID,
            "pnpm"
        )
    }

    func testUpdatesMarkOnlyAffectedEnvironmentDomains() {
        let presentation = popover(
            projection: project(WayfinderProjectionInput(updateCount: 3)).content,
            relatedRouteStages: [.apps, .packages]
        )

        XCTAssertTrue(presentation.showsPrimaryAction)
        XCTAssertEqual(presentation.primaryActionTitle.key, "app.wayfinder.action.review_plan")
        XCTAssertEqual(
            presentation.routeItems,
            [
                WayfinderPopoverRouteItem(stage: .system, tone: .current),
                WayfinderPopoverRouteItem(stage: .tools, tone: .current),
                WayfinderPopoverRouteItem(stage: .apps, tone: .updates),
                WayfinderPopoverRouteItem(stage: .packages, tone: .updates),
            ]
        )
    }

    func testOfflinePopoverUsesCachedRouteAndDisablesRefresh() {
        let presentation = popover(
            projection: project(WayfinderProjectionInput(networkAvailable: false)).content,
            relatedRouteStages: [.tools]
        )

        XCTAssertFalse(presentation.allowsRefresh)
        XCTAssertEqual(
            presentation.primaryActionTitle.key,
            "app.popover.wayfinder.action.view_saved_state"
        )
        XCTAssertEqual(
            presentation.routeItems.map(\.tone),
            Array(repeating: .cached, count: 4)
        )
    }

    func testFailureMarksAffectedDomainAndUsesRecoveryAction() {
        let presentation = popover(
            projection: project(WayfinderProjectionInput(failedTaskIDs: ["task"])).content,
            relatedRouteStages: [.apps]
        )

        XCTAssertEqual(
            presentation.primaryActionTitle.key,
            "app.popover.wayfinder.action.review_recovery"
        )
        XCTAssertEqual(
            presentation.routeItems.first(where: { $0.stage == .apps })?.tone,
            .error
        )
    }

    func testPopoverFixturesCoverApprovedStateSuite() throws {
        let now = Date(timeIntervalSince1970: 1_786_876_800)
        let fixtures = Dictionary(
            uniqueKeysWithValues: WayfinderPopoverFixtureName.allCases.map { name in
                (name, WayfinderPopoverFixtureProvider.fixture(named: name, now: now))
            }
        )

        XCTAssertEqual(
            WayfinderPopoverFixtureName.allCases.map(\.rawValue),
            ["healthy", "updates-ready", "running", "needs-review", "error", "offline"]
        )
        XCTAssertEqual(fixtures[.healthy]?.presentation.projection.condition, .healthy)
        XCTAssertEqual(
            fixtures[.updatesReady]?.presentation.projection.condition,
            .updatesReady(count: 4)
        )
        XCTAssertEqual(
            fixtures[.running]?.presentation.projection.condition,
            .activeWork(count: 1)
        )
        XCTAssertEqual(
            fixtures[.needsReview]?.presentation.projection.condition,
            .actionableFinding(count: 1)
        )
        XCTAssertEqual(
            fixtures[.error]?.presentation.projection.condition,
            .failedOrInterrupted(failed: 1, interrupted: 0)
        )
        XCTAssertEqual(fixtures[.offline]?.presentation.projection.condition, .offline)

        let progress = try XCTUnwrap(fixtures[.running]?.presentation.projection.progress)
        XCTAssertEqual(progress.completed, 5)
        XCTAssertEqual(progress.total, 12)
        XCTAssertFalse(try XCTUnwrap(fixtures[.offline]?.presentation.allowsRefresh))
    }

    func testPopoverFixturesPreserveAffectedRoutesAndFindingContext() throws {
        let now = Date(timeIntervalSince1970: 1_786_876_800)
        let updates = WayfinderPopoverFixtureProvider.fixture(named: .updatesReady, now: now)
        let needsReview = WayfinderPopoverFixtureProvider.fixture(named: .needsReview, now: now)

        XCTAssertEqual(
            updates.presentation.routeItems.filter { $0.tone == .updates }.map(\.stage),
            [.apps, .packages]
        )
        let toolsRoute = try XCTUnwrap(
            needsReview.presentation.routeItems.first { $0.stage == .tools }
        )
        XCTAssertEqual(toolsRoute.tone, .review)
        XCTAssertEqual(toolsRoute.managerID, "mise")
        XCTAssertEqual(
            needsReview.presentation.contextTitle.arguments,
            ["manager": "mise"]
        )
        XCTAssertEqual(
            needsReview.presentation.contextDetail.key,
            "app.inspector.multi_instance.attention_title"
        )
    }

    func testPopoverFixtureSelectionIsDebugOnlyAndRejectsUnknownNames() throws {
        let now = Date(timeIntervalSince1970: 1_786_876_800)

        XCTAssertFalse(WayfinderPopoverFixtureProvider.isActive(environment: [:]))
        XCTAssertNil(
            WayfinderPopoverFixtureProvider.active(
                environment: [WayfinderPopoverFixtureProvider.environmentKey: "unknown"],
                now: now
            )
        )

        let fixtureEnvironment = [
            WayfinderPopoverFixtureProvider.environmentKey: "  UPDATES-READY  "
        ]

        #if DEBUG
        let selected = try XCTUnwrap(
            WayfinderPopoverFixtureProvider.active(
                environment: fixtureEnvironment,
                now: now
            )
        )
        XCTAssertEqual(selected.name, .updatesReady)
        XCTAssertEqual(selected.schemaVersion, "1.0.0")
        #else
        XCTAssertNil(
            WayfinderPopoverFixtureProvider.active(
                environment: fixtureEnvironment,
                now: now
            )
        )
        #endif
    }

    func testPopoverFixtureProjectionIsDeterministicForReferenceDate() {
        let now = Date(timeIntervalSince1970: 1_786_876_800)

        XCTAssertEqual(
            WayfinderPopoverFixtureProvider.fixture(named: .running, now: now),
            WayfinderPopoverFixtureProvider.fixture(named: .running, now: now)
        )
    }

    func testPopoverBenchmarkIterationConfigurationIsDebugOnlyAndBounded() {
        XCTAssertNil(WayfinderPopoverBenchmarkConfiguration.iterations(environment: [:]))
        XCTAssertNil(
            WayfinderPopoverBenchmarkConfiguration.iterations(
                environment: [
                    WayfinderPopoverBenchmarkConfiguration.iterationsEnvironmentKey: "0"
                ]
            )
        )
        XCTAssertNil(
            WayfinderPopoverBenchmarkConfiguration.iterations(
                environment: [
                    WayfinderPopoverBenchmarkConfiguration.iterationsEnvironmentKey: "101"
                ]
            )
        )

        #if DEBUG
        XCTAssertEqual(
            WayfinderPopoverBenchmarkConfiguration.iterations(
                environment: [
                    WayfinderPopoverBenchmarkConfiguration.iterationsEnvironmentKey: " 30 "
                ]
            ),
            30
        )
        #else
        XCTAssertNil(
            WayfinderPopoverBenchmarkConfiguration.iterations(
                environment: [
                    WayfinderPopoverBenchmarkConfiguration.iterationsEnvironmentKey: "30"
                ]
            )
        )
        #endif
    }

    func testOrdinaryPopoverFootprintRemainsFixedAcrossStates() {
        XCTAssertEqual(WayfinderPopoverLayout.width, 400)
        XCTAssertEqual(WayfinderPopoverLayout.ordinaryHeight, 458)
        XCTAssertGreaterThan(
            WayfinderPopoverLayout.onboardingHeight,
            WayfinderPopoverLayout.ordinaryHeight
        )
    }

    private func project(
        _ input: WayfinderProjectionInput
    ) -> WayfinderPresentationProjection {
        WayfinderProjectionProjector.project(input, replacing: .initial)
    }

    private func popover(
        projection: WayfinderProjectionContent,
        relatedRouteStages: [WayfinderPopoverRouteStage] = [],
        relatedManagerIDsByStage: [WayfinderPopoverRouteStage: String] = [:],
        detectedManagerCount: Int = 0,
        findingContext: WayfinderPopoverFindingContext? = nil
    ) -> WayfinderPopoverPresentation {
        WayfinderPopoverPresentationProjector.content(
            for: WayfinderPopoverPresentationInput(
                projection: projection,
                relatedRouteStages: relatedRouteStages,
                relatedManagerIDsByStage: relatedManagerIDsByStage,
                detectedManagerCount: detectedManagerCount,
                findingContext: findingContext
            )
        )
    }
}
