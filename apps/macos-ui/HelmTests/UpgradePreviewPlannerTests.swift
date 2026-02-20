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
            step(id: "standard:four", order: 1, manager: "pip", authority: "standard", package: "four")
        ]

        let sorted = UpgradePreviewPlanner.sortedForExecution(steps)
        XCTAssertEqual(sorted.map(\.id), [
            "authoritative:three",
            "standard:four",
            "standard:one",
            "guarded:two"
        ])
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

    private func step(
        id: String,
        order: UInt64,
        manager: String,
        authority: String,
        package: String
    ) -> UpgradePreviewPlanner.PlanStep {
        UpgradePreviewPlanner.PlanStep(
            id: id,
            orderIndex: order,
            managerId: manager,
            authority: authority,
            packageName: package,
            reasonLabelKey: "service.task.label.upgrade.package"
        )
    }
}
