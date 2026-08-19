import XCTest

final class LibraryTableViewTests: XCTestCase {
    func testSelectionFindsConsolidatedRowForExactMemberPackage() {
        let rows = [
            makeRow(
                id: "ripgrep",
                representedPackageIDs: ["homebrew:ripgrep", "cargo:ripgrep"]
            ),
            makeRow(id: "fd", representedPackageIDs: ["homebrew:fd"]),
        ]

        XCTAssertEqual(
            LibraryTableSelectionPolicy.selectedRowID(
                forPackageID: "cargo:ripgrep",
                in: rows
            ),
            "ripgrep"
        )
    }

    func testSelectionReturnsNilWhenPackageIsNotVisible() {
        let rows = [makeRow(id: "ripgrep", representedPackageIDs: ["homebrew:ripgrep"])]

        XCTAssertNil(
            LibraryTableSelectionPolicy.selectedRowID(
                forPackageID: "cargo:ripgrep",
                in: rows
            )
        )
    }

    func testSelectionReturnsNilWithoutPackageIdentity() {
        XCTAssertNil(
            LibraryTableSelectionPolicy.selectedRowID(
                forPackageID: nil,
                in: [makeRow(id: "ripgrep", representedPackageIDs: ["homebrew:ripgrep"])]
            )
        )
    }

    func testRowKeepsExactInspectorAndActionTarget() {
        let row = makeRow(
            id: "ripgrep",
            representedPackageIDs: ["homebrew:ripgrep", "cargo:ripgrep"],
            selectedPackageID: "cargo:ripgrep",
            selectedManagerID: "cargo"
        )

        XCTAssertEqual(row.selectedPackageID, "cargo:ripgrep")
        XCTAssertEqual(row.selectedManagerID, "cargo")
        XCTAssertTrue(row.representedPackageIDs.contains(row.selectedPackageID))
        XCTAssertFalse(row.isRestartRequired)
    }

    private func makeRow(
        id: String,
        representedPackageIDs: [String],
        selectedPackageID: String? = nil,
        selectedManagerID: String = "homebrew_formula"
    ) -> LibraryTableRow {
        LibraryTableRow(
            id: id,
            representedPackageIDs: representedPackageIDs,
            selectedPackageID: selectedPackageID ?? representedPackageIDs[0],
            selectedManagerID: selectedManagerID,
            name: id,
            detail: nil,
            manager: selectedManagerID,
            currentVersion: "1.0.0",
            latestVersion: nil,
            status: "Installed",
            statusSymbolName: "checkmark.circle.fill",
            statusTone: .healthy,
            isPinned: false,
            isRestartRequired: false,
            action: nil
        )
    }
}
