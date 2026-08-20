import AppKit
import XCTest

final class LibraryTableProjectionAndReuseTests: XCTestCase {
    func testCoordinatorUsesRevisionWhileSelectionAndActionDriftRemainCorrect() throws {
        let initialRow = makeRow(id: "ripgrep", action: makeAction(identity: .install))
        let initialSnapshot = LibraryTableSnapshot(
            revision: LibraryTableModelRevision(namespace: "coordinator-test", generation: 1),
            rows: [initialRow]
        )
        let initialParent = makeTable(snapshot: initialSnapshot, selectedRowID: initialRow.id)
        let coordinator = initialParent.makeCoordinator()
        let tableView = makeNativeTable(coordinator: coordinator)

        XCTAssertTrue(coordinator.update(parent: initialParent))
        XCTAssertEqual(tableView.numberOfRows, 1)
        XCTAssertEqual(tableView.selectedRow, 0)

        let ignoredSameRevision = LibraryTableSnapshot(
            revision: initialSnapshot.revision,
            rows: [initialRow, makeRow(id: "fd")]
        )
        XCTAssertFalse(
            coordinator.update(
                parent: makeTable(snapshot: ignoredSameRevision, selectedRowID: nil)
            )
        )
        XCTAssertEqual(tableView.numberOfRows, 1)
        XCTAssertEqual(tableView.selectedRow, -1, "selection still synchronizes without a reload")

        let changedRow = makeRow(id: "ripgrep", action: makeAction(identity: .upgrade))
        let changedSnapshot = LibraryTableSnapshot(
            revision: LibraryTableModelRevision(namespace: "coordinator-test", generation: 2),
            rows: [changedRow, makeRow(id: "fd")]
        )
        XCTAssertTrue(
            coordinator.update(
                parent: makeTable(snapshot: changedSnapshot, selectedRowID: changedRow.id)
            )
        )
        XCTAssertEqual(tableView.numberOfRows, 2)
        let statusCell = try XCTUnwrap(
            coordinator.tableView(tableView, viewFor: tableView.tableColumns[3], row: 0)
        )
        let action = try XCTUnwrap(descendants(of: statusCell).compactMap { $0 as? NSButton }.first)
        XCTAssertEqual(action.accessibilityLabel(), "Upgrade")
        XCTAssertEqual(tableView.selectedRow, 0)
    }

    func testTwentyThousandRowsTraverseDistantViewportsAndClearReusedActionState() throws {
        let rows = (0..<20_000).map { index -> LibraryTableRow in
            if index < 100 {
                return makeRow(id: "package-\(index)", action: makeAction(identity: .upgrade))
            }
            if (9_900..<10_100).contains(index) {
                return makeRow(
                    id: "package-\(index)",
                    action: makeAction(identity: .upgrade, isInFlight: true)
                )
            }
            return makeRow(id: "package-\(index)")
        }
        let snapshot = LibraryTableSnapshot(
            revision: LibraryTableModelRevision(namespace: "reuse-test", generation: 1),
            rows: rows
        )
        let parent = makeTable(snapshot: snapshot, selectedRowID: nil)
        let coordinator = parent.makeCoordinator()
        let tableView = makeNativeTable(coordinator: coordinator, height: 600)
        let scrollView = LibraryTableScrollView(
            frame: NSRect(x: 0, y: 0, width: 900, height: 600)
        )
        scrollView.hasVerticalScroller = true
        scrollView.documentView = tableView
        let window = NSWindow(
            contentRect: scrollView.frame,
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )
        window.contentView = scrollView
        XCTAssertTrue(coordinator.update(parent: parent))

        let firstCells = visibleStatusCells(around: 0, tableView: tableView, scrollView: scrollView, window: window)
        XCTAssertFalse(firstCells.isEmpty)
        XCTAssertLessThanOrEqual(retainedRowViewCount(in: tableView), 20)
        for cell in firstCells {
            let button = try XCTUnwrap(descendants(of: cell).compactMap { $0 as? NSButton }.first)
            XCTAssertFalse(button.isHidden)
            XCTAssertEqual(button.accessibilityLabel(), "Upgrade")
            XCTAssertEqual(cell.accessibilityChildren()?.count, 1)
        }

        let middleCells = visibleStatusCells(around: 10_000, tableView: tableView, scrollView: scrollView, window: window)
        XCTAssertFalse(middleCells.isEmpty)
        XCTAssertFalse(viewIDs(firstCells).isDisjoint(with: viewIDs(middleCells)))
        XCTAssertLessThanOrEqual(retainedRowViewCount(in: tableView), 20)
        for cell in middleCells {
            let progress = try XCTUnwrap(descendants(of: cell).compactMap { $0 as? NSProgressIndicator }.first)
            XCTAssertFalse(progress.isHidden)
            XCTAssertEqual(cell.accessibilityLabel(), "Installed, Running")
            XCTAssertEqual(cell.accessibilityChildren()?.count, 0)
        }

        let lastCells = visibleStatusCells(around: 19_999, tableView: tableView, scrollView: scrollView, window: window)
        XCTAssertFalse(lastCells.isEmpty)
        XCTAssertFalse(
            viewIDs(firstCells + middleCells).isDisjoint(with: viewIDs(lastCells)),
            "distant traversal must exercise retained AppKit cell reuse"
        )
        XCTAssertLessThanOrEqual(retainedRowViewCount(in: tableView), 20)
        for cell in lastCells {
            let descendants = descendants(of: cell)
            let button = try XCTUnwrap(descendants.compactMap { $0 as? NSButton }.first)
            let progress = try XCTUnwrap(descendants.compactMap { $0 as? NSProgressIndicator }.first)
            XCTAssertTrue(button.isHidden)
            XCTAssertNil(button.accessibilityLabel())
            XCTAssertTrue(progress.isHidden)
            XCTAssertEqual(cell.accessibilityLabel(), "Installed")
            XCTAssertEqual(cell.accessibilityChildren()?.count, 0)
        }

        coordinator.detach()
        window.contentView = nil
    }

    private func makeTable(
        snapshot: LibraryTableSnapshot,
        selectedRowID: String?
    ) -> LibraryTableView {
        LibraryTableView(
            snapshot: snapshot,
            selectedRowID: selectedRowID,
            columnLabels: LibraryTableColumnLabels(
                package: "Package", manager: "Manager",
                version: "Version", status: "Status",
                currentVersion: "Current", latestVersion: "Latest",
                pinned: "Pinned", restartRequired: "Restart required",
                running: "Running", viewDetails: "View details"
            ),
            accessibilityLabel: "Library",
            focusRequest: nil,
            onSelectRow: { _ in },
            onClearSelection: {},
            onShowDetails: { _ in },
            onPerformAction: { _ in },
            onFulfillFocusRequest: { _ in }
        )
    }

    private func makeNativeTable(
        coordinator: LibraryTableView.Coordinator,
        height: CGFloat = 120
    ) -> LibraryNativeTableView {
        let tableView = LibraryNativeTableView(frame: NSRect(x: 0, y: 0, width: 900, height: height))
        tableView.dataSource = coordinator
        tableView.delegate = coordinator
        tableView.intercellSpacing = NSSize(width: 8, height: 2)
        LibraryTableLayoutPolicy.configure(in: tableView)
        coordinator.installColumns(in: tableView)
        coordinator.attach(tableView)
        return tableView
    }

    private func visibleStatusCells(
        around row: Int,
        tableView: LibraryNativeTableView,
        scrollView: LibraryTableScrollView,
        window: NSWindow
    ) -> [NSView] {
        let lastRowMaxY = tableView.rect(ofRow: tableView.numberOfRows - 1).maxY
        let maximumOffset = max(0, lastRowMaxY - scrollView.contentView.bounds.height)
        let requestedOffset = tableView.rect(ofRow: row).minY
        scrollView.contentView.scroll(to: NSPoint(x: 0, y: min(requestedOffset, maximumOffset)))
        scrollView.reflectScrolledClipView(scrollView.contentView)
        tableView.layoutSubtreeIfNeeded()
        scrollView.layoutSubtreeIfNeeded()
        window.displayIfNeeded()

        let visibleRows = tableView.rows(in: tableView.visibleRect)
        guard visibleRows.location != NSNotFound, visibleRows.length > 0 else { return [] }
        return (visibleRows.location..<(visibleRows.location + visibleRows.length)).compactMap {
            tableView.view(atColumn: 3, row: $0, makeIfNecessary: true)
        }
    }

    private func retainedRowViewCount(in tableView: NSTableView) -> Int {
        tableView.subviews.lazy.filter { $0 is NSTableRowView }.count
    }

    private func viewIDs(_ views: [NSView]) -> Set<ObjectIdentifier> {
        Set(views.map(ObjectIdentifier.init))
    }

    private func descendants(of view: NSView) -> [NSView] {
        view.subviews + view.subviews.flatMap(descendants(of:))
    }

    private func makeAction(
        identity: LibraryTableActionIdentity,
        isInFlight: Bool = false
    ) -> LibraryTableAction {
        LibraryTableAction(
            identity: identity,
            symbolName: "arrow.up.circle",
            title: identity.rawValue.capitalized,
            isEnabled: !isInFlight,
            isInFlight: isInFlight
        )
    }

    private func makeRow(
        id: String,
        action: LibraryTableAction? = nil
    ) -> LibraryTableRow {
        LibraryTableRow(
            id: id,
            representedPackageIDs: ["homebrew_formula:\(id)"],
            selectedPackageID: "homebrew_formula:\(id)",
            selectedManagerID: "homebrew_formula", name: id,
            detail: nil, manager: "Homebrew",
            currentVersion: "1.0.0", latestVersion: nil, status: "Installed",
            statusSymbolName: "checkmark.circle.fill",
            statusTone: .healthy,
            isPinned: false, isRestartRequired: false,
            action: action
        )
    }
}
