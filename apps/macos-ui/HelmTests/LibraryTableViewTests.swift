import AppKit
import XCTest

final class LibraryTableViewTests: XCTestCase {
    func testPerformanceBenchmarkIterationConfigurationIsDebugOnlyAndBounded() {
        XCTAssertNil(LibraryPerformanceBenchmarkConfiguration.iterations(environment: [:]))
        XCTAssertNil(
            LibraryPerformanceBenchmarkConfiguration.iterations(
                environment: [
                    LibraryPerformanceBenchmarkConfiguration.iterationsEnvironmentKey: "29"
                ]
            )
        )
        XCTAssertNil(
            LibraryPerformanceBenchmarkConfiguration.iterations(
                environment: [
                    LibraryPerformanceBenchmarkConfiguration.iterationsEnvironmentKey: "101"
                ]
            )
        )
        #if DEBUG
        XCTAssertEqual(
            LibraryPerformanceBenchmarkConfiguration.iterations(
                environment: [
                    LibraryPerformanceBenchmarkConfiguration.iterationsEnvironmentKey: " 30 "
                ]
            ),
            30
        )
        #else
        XCTAssertNil(
            LibraryPerformanceBenchmarkConfiguration.iterations(
                environment: [
                    LibraryPerformanceBenchmarkConfiguration.iterationsEnvironmentKey: "30"
                ]
            )
        )
        #endif
    }

    func testPerformanceSummaryUsesMedianNearestRankP95AndWorst() throws {
        let summary = try XCTUnwrap(
            LibraryPerformanceSampleSummary(samples: Array(1...30).map(Double.init))
        )

        XCTAssertEqual(summary.medianMilliseconds, 15.5)
        XCTAssertEqual(summary.p95Milliseconds, 29)
        XCTAssertEqual(summary.worstMilliseconds, 30)
        XCTAssertNil(LibraryPerformanceSampleSummary(samples: []))
    }

    func testIndexedSearchPolicyPreservesQueryAndManagerSemantics() {
        let member = LibraryPackageIndexMemberFacts(
            managerID: "cargo",
            status: "available",
            isPinned: false,
            normalizedIdentityKey: "ripgrep@nightly",
            managerSearchText: "cargo",
            summarySearchText: "fast recursive search"
        )

        XCTAssertTrue(matchesIndexedMember(member, query: "ripgrep"))
        XCTAssertTrue(
            matchesIndexedMember(
                member,
                query: "ripgrep@nightly-2026-08-19",
                normalizedQuery: "ripgrep@nightly"
            )
        )
        XCTAssertTrue(matchesIndexedMember(member, query: "cargo"))
        XCTAssertTrue(matchesIndexedMember(member, query: "recursive"))
        XCTAssertFalse(matchesIndexedMember(member, query: "missing"))
        XCTAssertFalse(matchesIndexedMember(member, query: "ripgrep", managerID: "npm"))
        XCTAssertFalse(
            matchesIndexedMember(member, query: "ripgrep", participatesInSearch: false)
        )
    }

    func testIndexedSearchPolicyPreservesPinnedAndStatusFilters() {
        let pinnedUpgrade = LibraryPackageIndexMemberFacts(
            managerID: "homebrew_formula",
            status: "upgradable",
            isPinned: true,
            normalizedIdentityKey: "ripgrep",
            managerSearchText: "homebrew",
            summarySearchText: ""
        )
        let unpinnedUpgrade = LibraryPackageIndexMemberFacts(
            managerID: pinnedUpgrade.managerID,
            status: pinnedUpgrade.status,
            isPinned: false,
            normalizedIdentityKey: pinnedUpgrade.normalizedIdentityKey,
            managerSearchText: pinnedUpgrade.managerSearchText,
            summarySearchText: pinnedUpgrade.summarySearchText
        )

        XCTAssertFalse(
            matchesIndexedMember(pinnedUpgrade, query: "", statusFilter: "upgradable")
        )
        XCTAssertTrue(
            matchesIndexedMember(unpinnedUpgrade, query: "", statusFilter: "upgradable")
        )
        XCTAssertTrue(
            matchesIndexedMember(pinnedUpgrade, query: "", statusFilter: "installed")
        )
        XCTAssertTrue(matchesIndexedMember(pinnedUpgrade, query: "", pinnedOnly: true))
        XCTAssertFalse(matchesIndexedMember(unpinnedUpgrade, query: "", pinnedOnly: true))
        XCTAssertFalse(
            matchesIndexedMember(unpinnedUpgrade, query: "", statusFilter: "available")
        )
    }

    func testSelectionFindsConsolidatedRowForExactMemberPackage() {
        let rows = [
            makeRow(
                id: "ripgrep",
                representedPackageIDs: ["homebrew_formula:ripgrep", "cargo:ripgrep"]
            ),
            makeRow(id: "fd", representedPackageIDs: ["homebrew_formula:fd"]),
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
        let rows = [makeRow(id: "ripgrep", representedPackageIDs: ["homebrew_formula:ripgrep"])]

        XCTAssertNil(
            LibraryTableSelectionPolicy.selectedRowID(
                forPackageID: "cargo:ripgrep",
                in: rows
            )
        )
    }

    func testExactMemberWinsBeforePreferredManager() {
        let target = PackageMemberSelectionPolicy.actionTarget(
            members: makePackageMembers(),
            orderedManagerIDs: ["homebrew_formula", "cargo"],
            preferredManagerID: "homebrew_formula",
            selectedPackageID: "cargo:ripgrep"
        )

        XCTAssertEqual(
            target,
            PackageMemberIdentity(packageID: "cargo:ripgrep", managerID: "cargo")
        )
    }

    func testInstallSelectionPrefersExactMemberUnlessManagerIsConstrained() {
        XCTAssertEqual(
            PackageMemberSelectionPolicy.initialInstallSelection(
                candidates: makePackageMembers(),
                managerConstraint: nil,
                preferredManagerID: "homebrew_formula",
                selectedPackageID: "cargo:ripgrep"
            ),
            PackageMemberSelection(managerID: "cargo", packageID: "cargo:ripgrep")
        )
        XCTAssertEqual(
            PackageMemberSelectionPolicy.initialInstallSelection(
                candidates: makePackageMembers(),
                managerConstraint: "homebrew_formula",
                preferredManagerID: "cargo",
                selectedPackageID: "cargo:ripgrep"
            ),
            PackageMemberSelection(
                managerID: "homebrew_formula",
                packageID: "homebrew_formula:ripgrep"
            )
        )
    }

    func testCommandResolutionFailsClosedWhenMemberOrActionDrifts() {
        let command = LibraryTableCommand(
            rowID: "ripgrep",
            packageID: "cargo:ripgrep",
            actionIdentity: .install
        )
        let stable = makeRow(
            id: "ripgrep",
            representedPackageIDs: ["homebrew_formula:ripgrep", "cargo:ripgrep"],
            selectedPackageID: "cargo:ripgrep",
            selectedManagerID: "cargo",
            action: makeAction(identity: .install)
        )
        let replacementMember = makeRow(
            id: "ripgrep",
            representedPackageIDs: stable.representedPackageIDs,
            selectedPackageID: "homebrew_formula:ripgrep",
            action: makeAction(identity: .install)
        )
        let replacementAction = makeRow(
            id: "ripgrep",
            representedPackageIDs: stable.representedPackageIDs,
            selectedPackageID: "cargo:ripgrep",
            selectedManagerID: "cargo",
            action: makeAction(identity: .upgrade)
        )
        let disabledAction = makeRow(
            id: "ripgrep",
            representedPackageIDs: stable.representedPackageIDs,
            selectedPackageID: "cargo:ripgrep",
            selectedManagerID: "cargo",
            action: makeAction(identity: .install, isEnabled: false)
        )

        XCTAssertEqual(
            LibraryTableCommandPolicy.resolvedRow(for: command, in: [stable.id: stable]),
            stable
        )
        XCTAssertNil(
            LibraryTableCommandPolicy.resolvedRow(
                for: command,
                in: [replacementMember.id: replacementMember]
            )
        )
        XCTAssertNil(
            LibraryTableCommandPolicy.resolvedRow(
                for: command,
                in: [replacementAction.id: replacementAction]
            )
        )
        XCTAssertNil(
            LibraryTableCommandPolicy.resolvedRow(
                for: command,
                in: [disabledAction.id: disabledAction]
            )
        )
    }

    func testAppKitRowConfigurationPreservesFiftyPointRows() {
        let tableView = LibraryNativeTableView(frame: NSRect(x: 0, y: 0, width: 700, height: 100))
        let dataSource = SingleRowDataSource()
        tableView.dataSource = dataSource
        tableView.addTableColumn(NSTableColumn(identifier: NSUserInterfaceItemIdentifier("package")))

        LibraryTableLayoutPolicy.configure(in: tableView)
        tableView.reloadData()

        XCTAssertFalse(tableView.autoresizingMask.contains(.width))
        XCTAssertEqual(tableView.rowSizeStyle, .custom)
        XCTAssertEqual(tableView.rowHeight, 50, accuracy: 0.1)
        XCTAssertGreaterThanOrEqual(tableView.rect(ofRow: 0).height, 50)
    }

    func testTwentyThousandRowTableRealizesOnlyVisibleViewport() throws {
        let rows = (0..<20_000).map { index in
            makeRow(
                id: "package-\(index)",
                representedPackageIDs: ["manager-\(index % 30):package-\(index)"]
            )
        }
        let parent = makeTable(rows: rows, selectedRowID: nil)
        let coordinator = parent.makeCoordinator()
        let tableView = LibraryNativeTableView(
            frame: NSRect(x: 0, y: 0, width: 900, height: 600)
        )
        tableView.dataSource = coordinator
        tableView.delegate = coordinator
        LibraryTableLayoutPolicy.configure(in: tableView)
        coordinator.installColumns(in: tableView)
        coordinator.attach(tableView)

        let scrollView = LibraryTableScrollView(
            frame: NSRect(x: 0, y: 0, width: 900, height: 600)
        )
        scrollView.documentView = tableView
        coordinator.update(parent: parent)
        scrollView.layoutSubtreeIfNeeded()

        XCTAssertEqual(tableView.numberOfRows, 20_000)
        let visibleRows = tableView.rows(in: scrollView.contentView.bounds)
        XCTAssertGreaterThan(visibleRows.length, 0)
        XCTAssertLessThan(visibleRows.length, 20)
        for row in visibleRows.location..<(visibleRows.location + visibleRows.length) {
            for column in tableView.tableColumns.indices {
                _ = tableView.view(atColumn: column, row: row, makeIfNecessary: true)
            }
        }

        XCTAssertNil(tableView.rowView(atRow: 19_999, makeIfNecessary: false))
        XCTAssertLessThan(tableView.subviews.count, 20)
    }

    func testConstrainedTableDocumentContainsTrailingActionColumn() {
        for width in [480, 520, 552, 800] as [CGFloat] {
            let tableView = makeNativeTable(width: width)
            let scrollView = LibraryTableScrollView(
                frame: NSRect(x: 0, y: 0, width: width, height: 120)
            )
            scrollView.hasHorizontalScroller = true
            scrollView.documentView = tableView
            scrollView.tile()
            scrollView.fitDocumentWidthToViewport()

            let trailingExtent = tableView.rect(ofColumn: tableView.numberOfColumns - 1).maxX
            XCTAssertGreaterThanOrEqual(tableView.frame.width, trailingExtent, "width: \(width)")
            if width <= 552 {
                XCTAssertGreaterThan(
                    tableView.frame.width,
                    scrollView.contentView.bounds.width,
                    "width: \(width)"
                )
            }

            scrollView.setFrameSize(NSSize(width: width + 1, height: 120))
            scrollView.tile()
            let resizedTrailingExtent = tableView.rect(
                ofColumn: tableView.numberOfColumns - 1
            ).maxX
            XCTAssertGreaterThanOrEqual(
                tableView.frame.width,
                resizedTrailingExtent,
                "resized width: \(width)"
            )
            if width <= 552 {
                XCTAssertGreaterThan(
                    tableView.frame.width,
                    scrollView.contentView.bounds.width,
                    "resized width: \(width)"
                )
            }
        }

        let tableView = makeNativeTable(width: 500)
        let scrollView = LibraryTableScrollView(frame: NSRect(x: 0, y: 0, width: 500, height: 120))
        scrollView.hasHorizontalScroller = true
        scrollView.documentView = tableView
        scrollView.fitDocumentWidthToViewport()
        let previousRequiredWidth = tableView.requiredContentWidth
        tableView.tableColumns[1].width += 80
        scrollView.fitDocumentWidthToViewport()
        XCTAssertGreaterThan(tableView.requiredContentWidth, previousRequiredWidth)
        XCTAssertGreaterThanOrEqual(
            tableView.frame.width,
            tableView.rect(ofColumn: tableView.numberOfColumns - 1).maxX
        )
    }

    func testAutoFittedPackageColumnDoesNotAdvertiseUserResize() throws {
        let parent = makeTable(rows: [], selectedRowID: nil)
        let coordinator = parent.makeCoordinator()
        let tableView = LibraryNativeTableView()

        coordinator.installColumns(in: tableView)

        let packageColumn = try XCTUnwrap(tableView.tableColumns.first)
        XCTAssertTrue(packageColumn.resizingMask.contains(.autoresizingMask))
        XCTAssertFalse(packageColumn.resizingMask.contains(.userResizingMask))
    }

    func testUserDeselectionClearsOwnerSelection() {
        let row = makeRow(id: "ripgrep", representedPackageIDs: ["homebrew_formula:ripgrep"])
        var clearCount = 0
        let parent = makeTable(rows: [row], selectedRowID: row.id) {
            clearCount += 1
        }
        let coordinator = parent.makeCoordinator()
        let tableView = LibraryNativeTableView(frame: NSRect(x: 0, y: 0, width: 700, height: 100))
        tableView.allowsEmptySelection = true
        tableView.dataSource = coordinator
        tableView.delegate = coordinator
        LibraryTableLayoutPolicy.configure(in: tableView)
        coordinator.installColumns(in: tableView)
        coordinator.attach(tableView)
        coordinator.update(parent: parent)

        XCTAssertEqual(tableView.selectedRow, 0)
        tableView.deselectAll(nil)

        XCTAssertEqual(tableView.selectedRow, -1)
        XCTAssertEqual(clearCount, 1)
    }

    func testSemanticCellsExposeFactsOnceAndOnlyExposeActionControl() throws {
        let row = makeRow(
            id: "ripgrep",
            representedPackageIDs: ["homebrew_formula:ripgrep"],
            detail: "Cached, Recommended",
            latestVersion: "2.0.0",
            isPinned: true,
            isRestartRequired: true,
            action: makeAction(identity: .upgrade, title: "Upgrade")
        )
        let parent = makeTable(rows: [row], selectedRowID: row.id)
        let coordinator = parent.makeCoordinator()
        let tableView = LibraryNativeTableView(frame: NSRect(x: 0, y: 0, width: 700, height: 100))
        tableView.dataSource = coordinator
        tableView.delegate = coordinator
        LibraryTableLayoutPolicy.configure(in: tableView)
        coordinator.installColumns(in: tableView)
        coordinator.attach(tableView)
        coordinator.update(parent: parent)

        let packageCell = try XCTUnwrap(
            coordinator.tableView(tableView, viewFor: tableView.tableColumns[0], row: 0)
        )
        let managerCell = try XCTUnwrap(
            coordinator.tableView(tableView, viewFor: tableView.tableColumns[1], row: 0)
        )
        let versionCell = try XCTUnwrap(
            coordinator.tableView(tableView, viewFor: tableView.tableColumns[2], row: 0)
        )
        let statusCell = try XCTUnwrap(
            coordinator.tableView(tableView, viewFor: tableView.tableColumns[3], row: 0)
        )

        XCTAssertEqual(packageCell.accessibilityLabel(), "ripgrep, Pinned, Cached, Recommended")
        XCTAssertEqual(packageCell.accessibilityChildren()?.count, 0)
        XCTAssertEqual(managerCell.accessibilityLabel(), "homebrew_formula")
        XCTAssertEqual(managerCell.accessibilityChildren()?.count, 0)
        XCTAssertEqual(
            versionCell.accessibilityLabel(),
            "Current 1.0.0, Latest 2.0.0, Restart required"
        )
        XCTAssertEqual(versionCell.accessibilityChildren()?.count, 0)
        XCTAssertEqual(statusCell.accessibilityLabel(), "Installed")
        let actionCell = try XCTUnwrap(statusCell.accessibilityChildren()?.first as? NSButtonCell)
        XCTAssertEqual(actionCell.image?.accessibilityDescription, "Upgrade")
    }

    func testAppKitAccessibilityProxyExportsSemanticFactsAndAction() throws {
        let row = makeRow(
            id: "ripgrep",
            representedPackageIDs: ["homebrew_formula:ripgrep"],
            detail: "Cached, Recommended",
            latestVersion: "2.0.0",
            isPinned: true,
            isRestartRequired: true,
            action: makeAction(identity: .upgrade, title: "Upgrade")
        )
        let parent = makeTable(rows: [row], selectedRowID: row.id)
        let coordinator = parent.makeCoordinator()
        let tableView = LibraryNativeTableView(
            frame: NSRect(x: 0, y: 0, width: 700, height: 100)
        )
        tableView.dataSource = coordinator
        tableView.delegate = coordinator
        LibraryTableLayoutPolicy.configure(in: tableView)
        coordinator.installColumns(in: tableView)
        coordinator.attach(tableView)
        coordinator.update(parent: parent)

        let scrollView = LibraryTableScrollView(
            frame: NSRect(x: 0, y: 0, width: 700, height: 100)
        )
        scrollView.documentView = tableView
        tableView.reloadData()
        scrollView.layoutSubtreeIfNeeded()
        for column in tableView.tableColumns.indices {
            _ = tableView.view(atColumn: column, row: 0, makeIfNecessary: true)
        }
        tableView.layoutSubtreeIfNeeded()

        let accessibilityRows = try XCTUnwrap(
            accessibilityAttribute(.rows, from: tableView) as? NSArray
        )
        XCTAssertEqual(accessibilityRows.count, 1)
        let accessibilityRow = try XCTUnwrap(accessibilityRows.firstObject)
        let accessibilityCells = try XCTUnwrap(
            accessibilityAttribute(.children, from: accessibilityRow) as? NSArray
        )
        XCTAssertEqual(accessibilityCells.count, 4)

        let packageCell = accessibilityCells[0]
        let versionCell = accessibilityCells[2]
        let statusCell = accessibilityCells[3]
        XCTAssertEqual(
            accessibilityAttribute(.role, from: packageCell) as? String,
            NSAccessibility.Role.cell.rawValue
        )
        XCTAssertEqual(
            accessibilityAttribute(.description, from: packageCell) as? String,
            "ripgrep, Pinned, Cached, Recommended"
        )
        XCTAssertNil(accessibilityAttribute(.value, from: packageCell))
        XCTAssertEqual(
            (accessibilityAttribute(.children, from: packageCell) as? NSArray)?.count,
            0
        )
        XCTAssertEqual(
            accessibilityAttribute(.description, from: versionCell) as? String,
            "Current 1.0.0, Latest 2.0.0, Restart required"
        )
        XCTAssertNil(accessibilityAttribute(.value, from: versionCell))
        XCTAssertEqual(
            (accessibilityAttribute(.children, from: versionCell) as? NSArray)?.count,
            0
        )

        let statusChildren = try XCTUnwrap(
            accessibilityAttribute(.children, from: statusCell) as? NSArray
        )
        XCTAssertEqual(statusChildren.count, 1)
        let action = try XCTUnwrap(statusChildren.firstObject)
        XCTAssertEqual(
            accessibilityAttribute(.role, from: action) as? String,
            NSAccessibility.Role.button.rawValue
        )
        XCTAssertEqual(
            accessibilityAttribute(.description, from: action) as? String,
            "Upgrade"
        )
    }

    func testInFlightSemanticStatusExposesRunningWithoutHiddenAction() throws {
        let row = makeRow(
            id: "ripgrep",
            representedPackageIDs: ["homebrew_formula:ripgrep"],
            action: makeAction(identity: .upgrade, title: "Upgrade", isInFlight: true)
        )
        let parent = makeTable(rows: [row], selectedRowID: row.id)
        let coordinator = parent.makeCoordinator()
        let tableView = LibraryNativeTableView(frame: NSRect(x: 0, y: 0, width: 700, height: 100))
        tableView.dataSource = coordinator
        tableView.delegate = coordinator
        coordinator.installColumns(in: tableView)
        coordinator.attach(tableView)
        coordinator.update(parent: parent)

        let statusCell = try XCTUnwrap(
            coordinator.tableView(tableView, viewFor: tableView.tableColumns[3], row: 0)
        )

        XCTAssertEqual(statusCell.accessibilityLabel(), "Installed, Running")
        XCTAssertEqual(statusCell.accessibilityChildren()?.count, 0)
    }

    private func makeTable(
        rows: [LibraryTableRow],
        selectedRowID: String?,
        onClearSelection: @escaping () -> Void = {}
    ) -> LibraryTableView {
        LibraryTableView(
            rows: rows,
            selectedRowID: selectedRowID,
            columnLabels: makeColumnLabels(),
            accessibilityLabel: "Library",
            focusRequest: nil,
            onSelectRow: { _ in },
            onClearSelection: onClearSelection,
            onShowDetails: { _ in },
            onPerformAction: { _ in },
            onFulfillFocusRequest: { _ in }
        )
    }

    private func matchesIndexedMember(
        _ member: LibraryPackageIndexMemberFacts,
        query: String,
        normalizedQuery: String? = nil,
        managerID: String? = nil,
        statusFilter: String? = nil,
        pinnedOnly: Bool = false,
        participatesInSearch: Bool = true
    ) -> Bool {
        LibraryPackageIndexMatchPolicy.matches(
            member,
            queryToken: query,
            normalizedQueryToken: normalizedQuery ?? query,
            managerID: managerID,
            statusFilter: statusFilter,
            pinnedOnly: pinnedOnly,
            managerParticipatesInSearch: { _ in participatesInSearch }
        )
    }

    private func accessibilityAttribute(
        _ attribute: NSAccessibility.Attribute,
        from object: Any
    ) -> Any? {
        guard let object = object as? NSObject else { return nil }
        let selector = NSSelectorFromString("accessibilityAttributeValue:")
        guard object.responds(to: selector) else { return nil }
        return object.perform(selector, with: attribute.rawValue)?.takeUnretainedValue()
    }

    private func makeNativeTable(width: CGFloat) -> LibraryNativeTableView {
        let tableView = LibraryNativeTableView(frame: NSRect(x: 0, y: 0, width: width, height: 100))
        LibraryTableLayoutPolicy.configure(in: tableView)
        tableView.intercellSpacing = NSSize(width: 8, height: 2)
        let dimensions: [(width: CGFloat, minimum: CGFloat)] = [
            (330, 180),
            (130, 90),
            (120, 90),
            (150, 112),
        ]
        for (index, dimension) in dimensions.enumerated() {
            let column = NSTableColumn(
                identifier: NSUserInterfaceItemIdentifier("column.\(index)")
            )
            column.minWidth = dimension.minimum
            column.width = dimension.width
            tableView.addTableColumn(column)
        }
        return tableView
    }

    private func makePackageMembers() -> [PackageMemberIdentity] {
        [
            PackageMemberIdentity(
                packageID: "homebrew_formula:ripgrep",
                managerID: "homebrew_formula"
            ),
            PackageMemberIdentity(packageID: "cargo:ripgrep", managerID: "cargo"),
        ]
    }

    private func makeColumnLabels() -> LibraryTableColumnLabels {
        LibraryTableColumnLabels(
            package: "Package",
            manager: "Manager",
            version: "Version",
            status: "Status",
            currentVersion: "Current",
            latestVersion: "Latest",
            pinned: "Pinned",
            restartRequired: "Restart required",
            running: "Running",
            viewDetails: "View details"
        )
    }

    private func makeAction(
        identity: LibraryTableActionIdentity,
        title: String = "Install",
        isEnabled: Bool = true,
        isInFlight: Bool = false
    ) -> LibraryTableAction {
        LibraryTableAction(
            identity: identity,
            symbolName: "arrow.down.circle",
            title: title,
            isEnabled: isEnabled,
            isInFlight: isInFlight
        )
    }

    private func makeRow(
        id: String,
        representedPackageIDs: [String],
        selectedPackageID: String? = nil,
        selectedManagerID: String = "homebrew_formula",
        detail: String? = nil,
        latestVersion: String? = nil,
        isPinned: Bool = false,
        isRestartRequired: Bool = false,
        action: LibraryTableAction? = nil
    ) -> LibraryTableRow {
        LibraryTableRow(
            id: id,
            representedPackageIDs: representedPackageIDs,
            selectedPackageID: selectedPackageID ?? representedPackageIDs[0],
            selectedManagerID: selectedManagerID,
            name: id,
            detail: detail,
            manager: selectedManagerID,
            currentVersion: "1.0.0",
            latestVersion: latestVersion,
            status: "Installed",
            statusSymbolName: "checkmark.circle.fill",
            statusTone: .healthy,
            isPinned: isPinned,
            isRestartRequired: isRestartRequired,
            action: action
        )
    }
}

private final class SingleRowDataSource: NSObject, NSTableViewDataSource {
    func numberOfRows(in tableView: NSTableView) -> Int {
        1
    }
}
