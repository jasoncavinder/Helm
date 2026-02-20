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
