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
}
