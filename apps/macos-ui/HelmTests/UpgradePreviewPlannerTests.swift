import XCTest

final class UpgradePreviewPlannerTests: XCTestCase {
    func testCountRespectsPinnedDisabledAndOsFilters() {
        let candidates = [
            UpgradePreviewPlanner.Candidate(managerId: "homebrew_formula", pinned: false),
            UpgradePreviewPlanner.Candidate(managerId: "homebrew_formula", pinned: true),
            UpgradePreviewPlanner.Candidate(managerId: "softwareupdate", pinned: false),
            UpgradePreviewPlanner.Candidate(managerId: "rustup", pinned: false),
        ]
        let managerEnabled = [
            "rustup": false,
        ]

        let count = UpgradePreviewPlanner.count(
            candidates: candidates,
            managerEnabled: managerEnabled,
            includePinned: false,
            allowOsUpdates: false,
            safeModeEnabled: false
        )

        XCTAssertEqual(count, 1)
    }

    func testCountExcludesOsUpdatesWhenSafeModeEnabledEvenIfAllowed() {
        let candidates = [
            UpgradePreviewPlanner.Candidate(managerId: "softwareupdate", pinned: false),
            UpgradePreviewPlanner.Candidate(managerId: "homebrew_formula", pinned: false),
        ]

        let count = UpgradePreviewPlanner.count(
            candidates: candidates,
            managerEnabled: [:],
            includePinned: false,
            allowOsUpdates: true,
            safeModeEnabled: true
        )

        XCTAssertEqual(count, 1)
    }

    func testBreakdownSortsByCountThenLocalizedName() {
        let candidates = [
            UpgradePreviewPlanner.Candidate(managerId: "alpha", pinned: false),
            UpgradePreviewPlanner.Candidate(managerId: "alpha", pinned: false),
            UpgradePreviewPlanner.Candidate(managerId: "gamma", pinned: false),
            UpgradePreviewPlanner.Candidate(managerId: "beta", pinned: false),
        ]

        let breakdown = UpgradePreviewPlanner.breakdown(
            candidates: candidates,
            managerEnabled: [:],
            includePinned: false,
            allowOsUpdates: true,
            safeModeEnabled: false,
            managerName: { $0.uppercased() }
        )

        XCTAssertEqual(
            breakdown,
            [
                .init(manager: "ALPHA", count: 2),
                .init(manager: "BETA", count: 1),
                .init(manager: "GAMMA", count: 1),
            ]
        )
    }

    func testSortedUpgradePlanStepsPrioritizesAuthorityThenOrderIndex() {
        let steps = [
            step(id: "standard:one", order: 5, manager: "npm", authority: "standard", package: "one"),
            step(id: "guarded:two", order: 1, manager: "softwareupdate", authority: "guarded", package: "two"),
            step(id: "authoritative:three", order: 99, manager: "mise", authority: "authoritative", package: "three"),
            step(id: "standard:four", order: 1, manager: "pip", authority: "standard", package: "four"),
            step(id: "interactive:five", order: 0, manager: "sparkle", authority: "interactive", package: "five")
        ]

        let sorted = UpgradePreviewPlanner.sortedForExecution(steps)
        XCTAssertEqual(sorted.map(\.id), [
            "authoritative:three",
            "standard:four",
            "standard:one",
            "guarded:two",
            "interactive:five"
        ])
    }

    func testAutomaticExecutionPolicyKeepsExternalSparkleInteractiveAndHelmOptIn() {
        XCTAssertFalse(
            UpgradePreviewPlanner.runsAutomatically(
                action: UpgradePreviewPlanner.externalSparkleAction,
                managerId: "sparkle",
                includeHelmSelfUpdate: true
            )
        )
        XCTAssertFalse(
            UpgradePreviewPlanner.runsAutomatically(
                action: UpgradePreviewPlanner.helmSelfUpdateAction,
                managerId: UpgradePreviewPlanner.helmSelfUpdateManagerId,
                includeHelmSelfUpdate: false
            )
        )
        XCTAssertTrue(
            UpgradePreviewPlanner.runsAutomatically(
                action: UpgradePreviewPlanner.helmSelfUpdateAction,
                managerId: UpgradePreviewPlanner.helmSelfUpdateManagerId,
                includeHelmSelfUpdate: true
            )
        )
        XCTAssertTrue(
            UpgradePreviewPlanner.runsAutomatically(
                action: "upgrade",
                managerId: "homebrew_formula",
                includeHelmSelfUpdate: false
            )
        )
    }

    func testInteractiveProjectionAddsHelmWhenBackendPlanIsEmpty() {
        let projected = UpgradePreviewPlanner.addingInteractiveUpdates(
            to: [],
            externalSparkleUpdates: [],
            helmUpdateVersion: "0.19.0-rc.3",
            externalSparkleReasonLabelKey: "external-sparkle",
            helmSelfUpdateReasonLabelKey: "helm-self-update"
        )

        XCTAssertEqual(projected.map(\.id), ["helm-self-update:Helm"])
        XCTAssertEqual(projected.first?.status, "not_included")
        XCTAssertEqual(projected.first?.reasonLabelArgs["version"], "0.19.0-rc.3")
    }

    func testInteractiveProjectionReplacesStaleLocalRowsWithoutDuplicatingBackendRows() {
        let backend = step(
            id: "npm:typescript",
            order: 0,
            manager: "npm",
            authority: "standard",
            package: "typescript"
        )
        let staleHelm = step(
            id: "helm-self-update:Helm",
            order: 1,
            manager: UpgradePreviewPlanner.helmSelfUpdateManagerId,
            authority: "interactive",
            action: UpgradePreviewPlanner.helmSelfUpdateAction,
            package: "Helm",
            status: "not_included"
        )

        let projected = UpgradePreviewPlanner.addingInteractiveUpdates(
            to: [backend, staleHelm],
            externalSparkleUpdates: [.init(id: "example", packageName: "Example")],
            helmUpdateVersion: "0.19.0-rc.4",
            externalSparkleReasonLabelKey: "external-sparkle",
            helmSelfUpdateReasonLabelKey: "helm-self-update"
        )

        XCTAssertEqual(projected.map(\.id), [
            "npm:typescript",
            "sparkle-external:example",
            "helm-self-update:Helm",
        ])
        XCTAssertEqual(projected.last?.reasonLabelArgs["version"], "0.19.0-rc.4")
    }

    func testScopedUpgradePlanStepsFiltersByManagerAndPackage() {
        let steps = [
            step(id: "npm:typescript", order: 0, manager: "npm", authority: "standard", package: "typescript"),
            step(id: "npm:eslint", order: 1, manager: "npm", authority: "standard", package: "eslint"),
            step(id: "pip:requests", order: 2, manager: "pip", authority: "standard", package: "requests")
        ]

        let managerScoped = UpgradePreviewPlanner.scopedForExecution(
            from: steps,
            managerScopeId: "npm",
            packageFilter: ""
        )
        XCTAssertEqual(managerScoped.map(\.id), ["npm:typescript", "npm:eslint"])

        let packageScoped = UpgradePreviewPlanner.scopedForExecution(
            from: steps,
            managerScopeId: UpgradePreviewPlanner.allManagersScopeId,
            packageFilter: "REQ"
        )
        XCTAssertEqual(packageScoped.map(\.id), ["pip:requests"])
    }

    func testPlanSelectionDefaultsNewStepsOnAndPreservesDeselection() {
        let initial = UpgradePreviewPlanner.reconcileSelection(
            selectedStepIds: [],
            knownStepIds: [],
            availableStepIds: ["mas:Pages", "npm:typescript"]
        )
        XCTAssertEqual(initial.selectedStepIds, ["mas:Pages", "npm:typescript"])

        let refreshed = UpgradePreviewPlanner.reconcileSelection(
            selectedStepIds: ["npm:typescript"],
            knownStepIds: initial.knownStepIds,
            availableStepIds: ["mas:Pages", "mas:Numbers", "npm:typescript"]
        )

        XCTAssertEqual(refreshed.selectedStepIds, ["mas:Numbers", "npm:typescript"])
        XCTAssertEqual(refreshed.knownStepIds, ["mas:Pages", "mas:Numbers", "npm:typescript"])
    }

    func testPlanSelectionDropsStepsNoLongerAvailable() {
        let refreshed = UpgradePreviewPlanner.reconcileSelection(
            selectedStepIds: ["mas:Pages", "npm:typescript"],
            knownStepIds: ["mas:Pages", "npm:typescript"],
            availableStepIds: ["npm:typescript"]
        )

        XCTAssertEqual(refreshed.selectedStepIds, ["npm:typescript"])
        XCTAssertEqual(refreshed.knownStepIds, ["npm:typescript"])
    }

    func testRiskSummaryUsesOnlySelectedCandidates() {
        let appStore = UpgradePreviewPlanner.RiskCandidate(managerId: "mas", packageName: "Pages")
        let homebrew = UpgradePreviewPlanner.RiskCandidate(
            managerId: "homebrew_formula",
            packageName: "ripgrep"
        )
        let macOS = UpgradePreviewPlanner.RiskCandidate(
            managerId: "softwareupdate",
            packageName: "macOS"
        )

        XCTAssertEqual(
            UpgradePreviewPlanner.riskSummary(
                for: [appStore, homebrew, macOS],
                restartRequiredCandidates: []
            ),
            .init(requiresElevatedPrivileges: true, mayRequireReboot: true)
        )
        XCTAssertEqual(
            UpgradePreviewPlanner.riskSummary(
                for: [homebrew],
                restartRequiredCandidates: [appStore]
            ),
            .init(requiresElevatedPrivileges: false, mayRequireReboot: false)
        )
    }

    func testRiskSummaryMatchesRestartRequirementByManagerAndPackage() {
        let selected = UpgradePreviewPlanner.RiskCandidate(managerId: "npm", packageName: "example")
        let otherManager = UpgradePreviewPlanner.RiskCandidate(
            managerId: "homebrew_formula",
            packageName: "example"
        )

        XCTAssertTrue(
            UpgradePreviewPlanner.riskSummary(
                for: [selected],
                restartRequiredCandidates: [selected]
            ).mayRequireReboot
        )
        XCTAssertFalse(
            UpgradePreviewPlanner.riskSummary(
                for: [selected],
                restartRequiredCandidates: [otherManager]
            ).mayRequireReboot
        )
    }

    func testShouldRunScopedStepHonorsRuntimeProjectionAndSafeMode() {
        XCTAssertTrue(
            UpgradePreviewPlanner.shouldRunScopedStep(
                status: "queued",
                hasProjectedTask: false,
                managerId: "npm",
                safeModeEnabled: false
            )
        )
        XCTAssertFalse(
            UpgradePreviewPlanner.shouldRunScopedStep(
                status: "queued",
                hasProjectedTask: true,
                managerId: "npm",
                safeModeEnabled: false
            )
        )
        XCTAssertFalse(
            UpgradePreviewPlanner.shouldRunScopedStep(
                status: "running",
                hasProjectedTask: true,
                managerId: "npm",
                safeModeEnabled: false
            )
        )
        XCTAssertFalse(
            UpgradePreviewPlanner.shouldRunScopedStep(
                status: "completed",
                hasProjectedTask: true,
                managerId: "npm",
                safeModeEnabled: false
            )
        )
        XCTAssertFalse(
            UpgradePreviewPlanner.shouldRunScopedStep(
                status: "failed",
                hasProjectedTask: false,
                managerId: "softwareupdate",
                safeModeEnabled: true
            )
        )
    }

    func testIsInFlightStatusTreatsQueuedWithoutProjectionAsNotRunning() {
        XCTAssertTrue(
            UpgradePreviewPlanner.isInFlightStatus(
                status: "running",
                hasProjectedTask: true
            )
        )
        XCTAssertTrue(
            UpgradePreviewPlanner.isInFlightStatus(
                status: "queued",
                hasProjectedTask: true
            )
        )
        XCTAssertFalse(
            UpgradePreviewPlanner.isInFlightStatus(
                status: "queued",
                hasProjectedTask: false
            )
        )
        XCTAssertFalse(
            UpgradePreviewPlanner.isInFlightStatus(
                status: "failed",
                hasProjectedTask: true
            )
        )
    }

    func testPlanStepIdPrefersExplicitThenManagerSpecificFallbacks() {
        XCTAssertEqual(
            UpgradePreviewPlanner.planStepId(
                managerId: "npm",
                labelArgs: ["plan_step_id": "npm:typescript", "package": "typescript"]
            ),
            "npm:typescript"
        )
        XCTAssertEqual(
            UpgradePreviewPlanner.planStepId(
                managerId: "softwareupdate",
                labelArgs: [:]
            ),
            "softwareupdate:__confirm_os_updates__"
        )
        XCTAssertEqual(
            UpgradePreviewPlanner.planStepId(
                managerId: "rustup",
                labelArgs: ["toolchain": "stable"]
            ),
            "rustup:stable"
        )
        XCTAssertEqual(
            UpgradePreviewPlanner.planStepId(
                managerId: "npm",
                labelArgs: ["package": "eslint"]
            ),
            "npm:eslint"
        )
        XCTAssertNil(
            UpgradePreviewPlanner.planStepId(
                managerId: "npm",
                labelArgs: [:]
            )
        )
    }

    func testProjectedTaskIdsForCancellationIncludesOnlyScopedInFlightTasks() {
        let overflownTaskId = UInt64(Int64.max) + 1
        let projections: [String: UpgradePreviewPlanner.ProjectedTaskState] = [
            "a": .init(taskId: 101, status: "queued"),
            "b": .init(taskId: 202, status: "running"),
            "c": .init(taskId: 303, status: "failed"),
            "d": .init(taskId: 404, status: "running"),
            "e": .init(taskId: overflownTaskId, status: "running"),
        ]

        let taskIds = UpgradePreviewPlanner.projectedTaskIdsForCancellation(
            scopedStepIds: ["a", "b", "c", "e"],
            projections: projections
        )

        XCTAssertEqual(taskIds, Set<Int64>([101, 202]))
    }

    func testWorkflowStartKnowsItsIdBeforeTheBackendReply() {
        var state = UpgradeWorkflowStartState()
        state.begin(workflowId: "upgrade-workflow-request-1")
        state.requestCancellation()

        XCTAssertEqual(state.workflowId, "upgrade-workflow-request-1")
        XCTAssertTrue(state.finish(workflowId: "upgrade-workflow-request-1"))
        XCTAssertFalse(state.isInFlight)
        XCTAssertFalse(state.cancellationPending)
    }

    func testWorkflowStartCompletionWithoutCancellationDoesNotRequestCancellation() {
        var state = UpgradeWorkflowStartState()
        state.begin(workflowId: "upgrade-workflow-request-2")

        XCTAssertFalse(state.finish(workflowId: "upgrade-workflow-request-2"))
        XCTAssertFalse(state.isInFlight)
    }

    func testWorkflowStatusReconciliationClearsImmediatelyWhenInactive() {
        var state = UpgradeWorkflowStatusReconciliationState()

        XCTAssertEqual(state.reconcile(isActive: false), .clearLocalState)
        XCTAssertEqual(state.indeterminateResultCount, 0)
    }

    func testWorkflowStatusReconciliationRecoversOnlyAfterRetryAndTimeBudget() {
        var state = UpgradeWorkflowStatusReconciliationState()
        let start = Date(timeIntervalSince1970: 1_000)

        XCTAssertEqual(state.reconcile(isActive: nil, now: start), .keepLocalState)
        XCTAssertEqual(
            state.reconcile(isActive: nil, now: start.addingTimeInterval(30)),
            .keepLocalState
        )
        XCTAssertEqual(
            state.reconcile(isActive: nil, now: start.addingTimeInterval(89)),
            .keepLocalState
        )
        XCTAssertEqual(
            state.reconcile(isActive: nil, now: start.addingTimeInterval(90)),
            .recoverLocalState
        )
        XCTAssertEqual(state.indeterminateResultCount, 0)
    }

    func testWorkflowStatusReconciliationResetsAfterActiveResult() {
        var state = UpgradeWorkflowStatusReconciliationState()
        let start = Date(timeIntervalSince1970: 1_000)

        _ = state.reconcile(isActive: nil, now: start)
        XCTAssertEqual(
            state.reconcile(isActive: true, now: start.addingTimeInterval(30)),
            .keepLocalState
        )
        XCTAssertEqual(state.indeterminateResultCount, 0)
        XCTAssertEqual(
            state.reconcile(isActive: nil, now: start.addingTimeInterval(120)),
            .keepLocalState
        )
    }

    private func step(
        id: String,
        order: UInt64,
        manager: String,
        authority: String,
        action: String = "upgrade",
        package: String,
        status: String = "queued"
    ) -> UpgradePreviewPlanner.PlanStep {
        UpgradePreviewPlanner.PlanStep(
            id: id,
            orderIndex: order,
            managerId: manager,
            authority: authority,
            action: action,
            packageName: package,
            reasonLabelKey: "service.task.label.upgrade.package",
            reasonLabelArgs: [:],
            status: status
        )
    }
}

final class PackageConsolidationPolicyTests: XCTestCase {
    func testSparkleSnapshotReplacementKeyDistinguishesSameNameBundlePaths() {
        let applicationsKey = PackageConsolidationPolicy.snapshotReplacementKey(
            managerId: "sparkle",
            packageName: "Example",
            packageIdentifier: "/Applications/Example.app"
        )
        let userApplicationsKey = PackageConsolidationPolicy.snapshotReplacementKey(
            managerId: "sparkle",
            packageName: "Example",
            packageIdentifier: "/Users/test/Applications/Example.app"
        )

        XCTAssertNotEqual(applicationsKey, userApplicationsKey)
        XCTAssertEqual(
            applicationsKey,
            PackageConsolidationPolicy.snapshotReplacementKey(
                managerId: "Sparkle",
                packageName: " example ",
                packageIdentifier: " /Applications/Example.app "
            )
        )
    }

    func testSnapshotReplacementKeyKeepsExistingNameIdentityForOtherManagers() {
        XCTAssertEqual(
            PackageConsolidationPolicy.snapshotReplacementKey(
                managerId: "npm",
                packageName: "typescript",
                packageIdentifier: "/one"
            ),
            PackageConsolidationPolicy.snapshotReplacementKey(
                managerId: "npm",
                packageName: "typescript",
                packageIdentifier: "/two"
            )
        )
    }

    func testSparkleInstanceLabelShowsAbbreviatedBundlePath() {
        XCTAssertEqual(
            PackageConsolidationPolicy.instanceDisambiguationLabel(
                managerId: "sparkle",
                packageIdentifier: "/Applications/Example.app"
            ),
            "/Applications/Example.app"
        )
        XCTAssertNil(
            PackageConsolidationPolicy.instanceDisambiguationLabel(
                managerId: "npm",
                packageIdentifier: "/Applications/Example.app"
            )
        )
    }

    func testStatusRankPrioritizesUpgradableThenInstalledThenAvailable() {
        XCTAssertLessThan(
            PackageConsolidationPolicy.statusRank("upgradable"),
            PackageConsolidationPolicy.statusRank("installed")
        )
        XCTAssertLessThan(
            PackageConsolidationPolicy.statusRank("installed"),
            PackageConsolidationPolicy.statusRank("available")
        )
    }

    func testSortedManagerIdsOrdersByLocalizedDisplayNameAndDeduplicates() {
        let sorted = PackageConsolidationPolicy.sortedManagerIds(
            ["pnpm", "npm", "pnpm", "brew"]
        ) { managerId in
            switch managerId {
            case "brew": return "Homebrew"
            case "npm": return "npm"
            case "pnpm": return "pnpm"
            default: return managerId
            }
        }

        XCTAssertEqual(sorted, ["brew", "npm", "pnpm"])
    }

    func testSortedManagerIdsUsesPriorityRankBeforeLocalizedName() {
        let sorted = PackageConsolidationPolicy.sortedManagerIds(
            ["guarded", "authoritative", "standard"],
            localizedManagerName: { _ in "zzz" },
            priorityRank: { managerId in
                switch managerId {
                case "authoritative": return 0
                case "standard": return 1
                case "guarded": return 2
                default: return Int.max
                }
            }
        )

        XCTAssertEqual(sorted, ["authoritative", "standard", "guarded"])
    }

    func testShouldPreferFavorsPinnedAndRestartRequiredAfterStatusRank() {
        XCTAssertTrue(
            PackageConsolidationPolicy.shouldPrefer(
                lhsStatus: "installed",
                rhsStatus: "installed",
                lhsPinned: true,
                rhsPinned: false,
                lhsRestartRequired: false,
                rhsRestartRequired: false,
                lhsManagerId: "a",
                rhsManagerId: "b",
                localizedManagerName: { $0 }
            )
        )

        XCTAssertTrue(
            PackageConsolidationPolicy.shouldPrefer(
                lhsStatus: "installed",
                rhsStatus: "installed",
                lhsPinned: false,
                rhsPinned: false,
                lhsRestartRequired: true,
                rhsRestartRequired: false,
                lhsManagerId: "a",
                rhsManagerId: "b",
                localizedManagerName: { $0 }
            )
        )
    }

    func testShouldPreferUsesPriorityRankWhenStatusPinnedAndRestartAreEqual() {
        XCTAssertTrue(
            PackageConsolidationPolicy.shouldPrefer(
                lhsStatus: "installed",
                rhsStatus: "installed",
                lhsPinned: false,
                rhsPinned: false,
                lhsRestartRequired: false,
                rhsRestartRequired: false,
                lhsManagerId: "authoritative",
                rhsManagerId: "guarded",
                localizedManagerName: { _ in "zzz" },
                priorityRank: { managerId in managerId == "authoritative" ? 0 : 2 }
            )
        )
    }

    func testPreferredManagerIdUsesPreferredWhenAvailable() {
        let selected = PackageConsolidationPolicy.preferredManagerId(
            managerIds: ["homebrew_formula", "pip"],
            preferredManagerId: "pip"
        )

        XCTAssertEqual(selected, "pip")
    }

    func testPreferredManagerIdFallsBackToFirstManagerWhenPreferredMissing() {
        let selected = PackageConsolidationPolicy.preferredManagerId(
            managerIds: ["homebrew_formula", "pip"],
            preferredManagerId: "cargo"
        )

        XCTAssertEqual(selected, "homebrew_formula")
    }
}

final class PackageActionTrackingTests: XCTestCase {
    func testPackageNameFromVersionedPackageId() {
        XCTAssertEqual(
            PackageActionTracking.packageNameFromPackageId("homebrew_formula:zig::0.14.0"),
            "zig"
        )
    }

    func testPackageNameFromStablePackageId() {
        XCTAssertEqual(
            PackageActionTracking.packageNameFromPackageId("pip:certifi"),
            "certifi"
        )
    }

    func testInFlightInstallNamesUsesTrackedNamesWhenCurrentSnapshotNoLongerContainsPackageId() {
        let names = PackageActionTracking.inFlightInstallNames(
            installActionPackageIds: ["homebrew_formula:zig::0.13.0"],
            packageNameById: [:],
            trackedNamesByPackageId: ["homebrew_formula:zig::0.13.0": "zig"]
        )

        XCTAssertEqual(names, ["zig"])
    }

    func testInFlightInstallNamesFallsBackToPackageIdParsingWhenTrackedNameMissing() {
        let names = PackageActionTracking.inFlightInstallNames(
            installActionPackageIds: ["homebrew_formula:zig::0.13.0"],
            packageNameById: [:],
            trackedNamesByPackageId: [:]
        )

        XCTAssertEqual(names, ["zig"])
    }
}

final class UpgradePlanCompletionTrackerTests: XCTestCase {
    func testExternalSparklePackageIdPreservesPackageIdentity() {
        XCTAssertEqual(
            UpgradePreviewPlanner.externalSparklePackageId(
                stepId: "sparkle-external:sparkle:/Applications/Example.app"
            ),
            "sparkle:/Applications/Example.app"
        )
        XCTAssertNil(UpgradePreviewPlanner.externalSparklePackageId(stepId: "npm:example"))
    }

    func testNormalCompletionKeepsOnlyStillAvailableInteractiveSteps() {
        var tracker = UpgradePlanCompletionTracker()
        tracker.begin(
            workflowId: "workflow",
            externalSparkleStepIds: ["sparkle-external:one", "sparkle-external:two"]
        )
        tracker.markAccepted(workflowId: "workflow")

        let completion = tracker.finish(
            workflowId: "workflow",
            currentExternalSparkleStepIds: ["sparkle-external:two", "sparkle-external:three"]
        )

        XCTAssertEqual(completion?.remainingExternalSparkleStepIds, ["sparkle-external:two"])
        XCTAssertEqual(completion?.remainingInteractiveCount, 1)
        XCTAssertEqual(completion?.completedNormally, true)
    }

    func testCancelledAndInterruptedWorkflowsDoNotCompleteNormally() {
        var cancelledTracker = UpgradePlanCompletionTracker()
        cancelledTracker.begin(workflowId: "cancelled", externalSparkleStepIds: [])
        cancelledTracker.markAccepted(workflowId: "cancelled")
        cancelledTracker.markCancelled(workflowId: "cancelled")
        XCTAssertEqual(
            cancelledTracker.finish(
                workflowId: "cancelled",
                currentExternalSparkleStepIds: []
            )?.completedNormally,
            false
        )

        var interruptedTracker = UpgradePlanCompletionTracker()
        interruptedTracker.begin(workflowId: "interrupted", externalSparkleStepIds: [])
        interruptedTracker.markObservedActive(workflowId: "interrupted")
        XCTAssertEqual(
            interruptedTracker.finish(
                workflowId: "interrupted",
                currentExternalSparkleStepIds: [],
                forceInterrupted: true
            )?.completedNormally,
            false
        )
    }
}
