import AppKit
import SwiftUI

enum LibraryTableStatusTone: Equatable {
    case healthy
    case updatesReady
    case available
    case neutral
}

struct LibraryTableAction: Equatable {
    let identity: LibraryTableActionIdentity
    let symbolName: String
    let title: String
    let isEnabled: Bool
    let isInFlight: Bool
}

enum LibraryTableActionIdentity: String, Equatable {
    case install
    case unpin
    case upgrade
    case openApplication
    case researchInstall
}

struct LibraryTableRow: Equatable, Identifiable {
    let id: String
    let representedPackageIDs: [String]
    let selectedPackageID: String
    let selectedManagerID: String
    let name: String
    let detail: String?
    let manager: String
    let currentVersion: String
    let latestVersion: String?
    let status: String
    let statusSymbolName: String
    let statusTone: LibraryTableStatusTone
    let isPinned: Bool
    let isRestartRequired: Bool
    let action: LibraryTableAction?
}

struct LibraryTableColumnLabels: Equatable {
    let package: String
    let manager: String
    let version: String
    let status: String
    let currentVersion: String
    let latestVersion: String
    let pinned: String
    let restartRequired: String
    let running: String
    let viewDetails: String
}

struct LibraryTableFocusRequest: Equatable {
    let requestID: Int
    let rowID: String
}

enum LibraryTableSelectionPolicy {
    static func selectedRowID(
        forPackageID packageID: String?,
        in rows: [LibraryTableRow]
    ) -> String? {
        guard let packageID else { return nil }
        return rows.first(where: { $0.representedPackageIDs.contains(packageID) })?.id
    }
}

struct LibraryTableCommand: Equatable {
    let rowID: String
    let packageID: String
    let actionIdentity: LibraryTableActionIdentity?
}

enum LibraryTableCommandPolicy {
    static func resolvedRow(
        for command: LibraryTableCommand,
        in rowsByID: [String: LibraryTableRow]
    ) -> LibraryTableRow? {
        guard let row = rowsByID[command.rowID],
              row.selectedPackageID == command.packageID else {
            return nil
        }
        guard let actionIdentity = command.actionIdentity else { return row }
        guard let action = row.action,
              action.identity == actionIdentity,
              action.isEnabled,
              !action.isInFlight else {
            return nil
        }
        return row
    }
}

enum LibraryTableLayoutPolicy {
    static let rowHeight: CGFloat = 50

    static func configure(in tableView: NSTableView) {
        tableView.autoresizingMask = []
        tableView.style = .plain
        tableView.rowSizeStyle = .custom
        tableView.rowHeight = rowHeight
    }

    static func layoutOverhead(in tableView: NSTableView) -> CGFloat {
        guard let lastColumnIndex = tableView.tableColumns.indices.last else { return 0 }
        let columnWidth = tableView.tableColumns.reduce(CGFloat.zero) { $0 + $1.width }
        let measuredExtent = tableView.rect(ofColumn: lastColumnIndex).maxX
        return max(0, measuredExtent - columnWidth)
    }

    static func requiredContentWidth(in tableView: NSTableView) -> CGFloat {
        tableView.tableColumns.reduce(CGFloat.zero) { $0 + $1.width }
            + layoutOverhead(in: tableView)
    }

    static func fitLeadingColumn(in tableView: NSTableView, to viewportWidth: CGFloat) {
        guard viewportWidth > 0, let leadingColumn = tableView.tableColumns.first else { return }
        let fixedWidth = tableView.tableColumns.dropFirst().reduce(CGFloat.zero) { $0 + $1.width }
        let leadingWidth = max(
            leadingColumn.minWidth,
            viewportWidth - fixedWidth - layoutOverhead(in: tableView)
        )
        guard abs(leadingColumn.width - leadingWidth) > 0.5 else { return }
        leadingColumn.width = leadingWidth
    }
}

enum LibraryTableAccessibilityPolicy {
    static func packageDescription(
        for row: LibraryTableRow,
        labels: LibraryTableColumnLabels
    ) -> String {
        var parts = [row.name]
        if row.isPinned {
            parts.append(labels.pinned)
        }
        if let detail = row.detail, !detail.isEmpty {
            parts.append(detail)
        }
        return parts.joined(separator: ", ")
    }

    static func versionDescription(
        for row: LibraryTableRow,
        labels: LibraryTableColumnLabels
    ) -> String {
        var parts = ["\(labels.currentVersion) \(row.currentVersion)"]
        if let latestVersion = row.latestVersion {
            parts.append("\(labels.latestVersion) \(latestVersion)")
        }
        if row.isRestartRequired {
            parts.append(labels.restartRequired)
        }
        return parts.joined(separator: ", ")
    }
}

struct LibraryTableView: NSViewRepresentable {
    let rows: [LibraryTableRow]
    let selectedRowID: String?
    let columnLabels: LibraryTableColumnLabels
    let accessibilityLabel: String
    let focusRequest: LibraryTableFocusRequest?
    let onSelectRow: (LibraryTableRow) -> Void
    let onClearSelection: () -> Void
    let onShowDetails: (LibraryTableRow) -> Void
    let onPerformAction: (LibraryTableRow) -> Void
    let onFulfillFocusRequest: (Int) -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(parent: self)
    }

    func makeNSView(context: Context) -> NSScrollView {
        let tableView = LibraryNativeTableView()
        tableView.dataSource = context.coordinator
        tableView.delegate = context.coordinator
        tableView.allowsEmptySelection = true
        tableView.allowsMultipleSelection = false
        tableView.allowsColumnReordering = false
        tableView.allowsColumnResizing = true
        tableView.allowsTypeSelect = true
        tableView.backgroundColor = .clear
        tableView.columnAutoresizingStyle = .noColumnAutoresizing
        tableView.gridStyleMask = []
        tableView.intercellSpacing = NSSize(width: 8, height: 2)
        tableView.selectionHighlightStyle = .regular
        LibraryTableLayoutPolicy.configure(in: tableView)
        tableView.usesAlternatingRowBackgroundColors = true
        tableView.setAccessibilityLabel(accessibilityLabel)
        tableView.contextMenuProvider = { [weak coordinator = context.coordinator] rowIndex in
            coordinator?.contextMenu(forRowAt: rowIndex)
        }
        tableView.didMoveToWindow = { [weak coordinator = context.coordinator] in
            coordinator?.fulfillFocusRequestIfPossible()
        }

        context.coordinator.installColumns(in: tableView)

        let scrollView = LibraryTableScrollView()
        scrollView.autohidesScrollers = true
        scrollView.borderType = .noBorder
        scrollView.documentView = tableView
        scrollView.drawsBackground = false
        scrollView.hasHorizontalScroller = true
        scrollView.hasVerticalScroller = true
        scrollView.scrollerStyle = .overlay

        context.coordinator.attach(tableView)
        context.coordinator.update(parent: self)
        return scrollView
    }

    func updateNSView(_ scrollView: NSScrollView, context: Context) {
        context.coordinator.update(parent: self)
        (scrollView as? LibraryTableScrollView)?.fitDocumentWidthToViewport()
    }

    static func dismantleNSView(_ scrollView: NSScrollView, coordinator: Coordinator) {
        coordinator.detach()
    }

    final class Coordinator: NSObject, NSTableViewDataSource, NSTableViewDelegate {
        private enum ColumnID {
            static let package = NSUserInterfaceItemIdentifier("library.package")
            static let manager = NSUserInterfaceItemIdentifier("library.manager")
            static let version = NSUserInterfaceItemIdentifier("library.version")
            static let status = NSUserInterfaceItemIdentifier("library.status")
        }

        private final class RowActionButton: NSButton {
            var command: LibraryTableCommand?
        }

        private final class RowMenuItem: NSMenuItem {
            var command: LibraryTableCommand?
        }

        private class SemanticTableCellView: NSTableCellView {
            var semanticAccessibilityLabel: String?
            var semanticAccessibilityChildren: [Any] = []

            override func accessibilityLabel() -> String? {
                semanticAccessibilityLabel
            }

            override func accessibilityChildren() -> [Any]? {
                semanticAccessibilityChildren
            }
        }

        private final class PackageCell: SemanticTableCellView {
            let statusImage = NSImageView()
            let titleLabel = NSTextField(labelWithString: "")
            let pinImage = NSImageView()
            let detailLabel = NSTextField(labelWithString: "")

            override init(frame frameRect: NSRect) {
                super.init(frame: frameRect)

                statusImage.imageScaling = .scaleProportionallyDown
                statusImage.translatesAutoresizingMaskIntoConstraints = false
                statusImage.setContentHuggingPriority(.required, for: .horizontal)
                statusImage.setAccessibilityElement(false)

                titleLabel.font = .systemFont(ofSize: NSFont.systemFontSize, weight: .medium)
                titleLabel.lineBreakMode = .byTruncatingTail
                titleLabel.maximumNumberOfLines = 1
                titleLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

                pinImage.image = NSImage(systemSymbolName: "pin.fill", accessibilityDescription: "")
                pinImage.contentTintColor = .secondaryLabelColor
                pinImage.imageScaling = .scaleProportionallyDown
                pinImage.translatesAutoresizingMaskIntoConstraints = false
                pinImage.setContentHuggingPriority(.required, for: .horizontal)
                pinImage.setAccessibilityElement(false)

                detailLabel.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
                detailLabel.textColor = .secondaryLabelColor
                detailLabel.lineBreakMode = .byTruncatingTail
                detailLabel.maximumNumberOfLines = 1
                detailLabel.setAccessibilityElement(false)

                let titleStack = NSStackView(views: [titleLabel, pinImage])
                titleStack.orientation = .horizontal
                titleStack.alignment = .centerY
                titleStack.spacing = 5

                let textStack = NSStackView(views: [titleStack, detailLabel])
                textStack.orientation = .vertical
                textStack.alignment = .leading
                textStack.spacing = 1

                let contentStack = NSStackView(views: [statusImage, textStack])
                contentStack.orientation = .horizontal
                contentStack.alignment = .centerY
                contentStack.spacing = 8
                contentStack.translatesAutoresizingMaskIntoConstraints = false
                addSubview(contentStack)

                NSLayoutConstraint.activate([
                    statusImage.widthAnchor.constraint(equalToConstant: 18),
                    statusImage.heightAnchor.constraint(equalToConstant: 18),
                    pinImage.widthAnchor.constraint(equalToConstant: 11),
                    pinImage.heightAnchor.constraint(equalToConstant: 11),
                    contentStack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 6),
                    contentStack.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -4),
                    contentStack.centerYAnchor.constraint(equalTo: centerYAnchor),
                ])
                textField = titleLabel
            }

            @available(*, unavailable)
            required init?(coder: NSCoder) {
                fatalError("init(coder:) has not been implemented")
            }
        }

        private final class VersionCell: SemanticTableCellView {
            let primaryLabel = NSTextField(labelWithString: "")
            let secondaryLabel = NSTextField(labelWithString: "")
            let restartImage = NSImageView()

            override init(frame frameRect: NSRect) {
                super.init(frame: frameRect)

                primaryLabel.font = .monospacedSystemFont(
                    ofSize: NSFont.smallSystemFontSize,
                    weight: .regular
                )
                primaryLabel.lineBreakMode = .byTruncatingMiddle
                primaryLabel.maximumNumberOfLines = 1
                secondaryLabel.font = .monospacedSystemFont(
                    ofSize: NSFont.smallSystemFontSize - 1,
                    weight: .regular
                )
                secondaryLabel.textColor = .secondaryLabelColor
                secondaryLabel.lineBreakMode = .byTruncatingMiddle
                secondaryLabel.maximumNumberOfLines = 1
                secondaryLabel.setAccessibilityElement(false)

                restartImage.image = NSImage(
                    systemSymbolName: "arrow.triangle.2.circlepath",
                    accessibilityDescription: ""
                )
                restartImage.contentTintColor = .systemOrange
                restartImage.imageScaling = .scaleProportionallyDown
                restartImage.translatesAutoresizingMaskIntoConstraints = false
                restartImage.setAccessibilityElement(false)

                let versionStack = NSStackView(views: [primaryLabel, secondaryLabel])
                versionStack.orientation = .vertical
                versionStack.alignment = .leading
                versionStack.spacing = 1

                let contentStack = NSStackView(views: [versionStack, restartImage])
                contentStack.orientation = .horizontal
                contentStack.alignment = .centerY
                contentStack.spacing = 5
                contentStack.translatesAutoresizingMaskIntoConstraints = false
                addSubview(contentStack)

                NSLayoutConstraint.activate([
                    restartImage.widthAnchor.constraint(equalToConstant: 13),
                    restartImage.heightAnchor.constraint(equalToConstant: 13),
                    contentStack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 4),
                    contentStack.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -4),
                    contentStack.centerYAnchor.constraint(equalTo: centerYAnchor),
                ])
                textField = primaryLabel
            }

            @available(*, unavailable)
            required init?(coder: NSCoder) {
                fatalError("init(coder:) has not been implemented")
            }
        }

        private final class StatusCell: SemanticTableCellView {
            let statusLabel = NSTextField(labelWithString: "")
            let progressIndicator = NSProgressIndicator()
            let actionButton = RowActionButton(title: "", target: nil, action: nil)

            override init(frame frameRect: NSRect) {
                super.init(frame: frameRect)

                statusLabel.font = .systemFont(ofSize: NSFont.smallSystemFontSize, weight: .medium)
                statusLabel.lineBreakMode = .byTruncatingTail
                statusLabel.maximumNumberOfLines = 1
                statusLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

                progressIndicator.controlSize = .small
                progressIndicator.isIndeterminate = true
                progressIndicator.style = .spinning
                progressIndicator.translatesAutoresizingMaskIntoConstraints = false

                actionButton.bezelStyle = .texturedRounded
                actionButton.controlSize = .small
                actionButton.imagePosition = .imageOnly
                actionButton.translatesAutoresizingMaskIntoConstraints = false

                let stack = NSStackView(views: [statusLabel, progressIndicator, actionButton])
                stack.orientation = .horizontal
                stack.alignment = .centerY
                stack.spacing = 6
                stack.translatesAutoresizingMaskIntoConstraints = false
                addSubview(stack)

                NSLayoutConstraint.activate([
                    progressIndicator.widthAnchor.constraint(equalToConstant: 16),
                    progressIndicator.heightAnchor.constraint(equalToConstant: 16),
                    actionButton.widthAnchor.constraint(equalToConstant: 28),
                    actionButton.heightAnchor.constraint(equalToConstant: 24),
                    stack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 4),
                    stack.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -6),
                    stack.centerYAnchor.constraint(equalTo: centerYAnchor),
                ])
                textField = statusLabel
            }

            @available(*, unavailable)
            required init?(coder: NSCoder) {
                fatalError("init(coder:) has not been implemented")
            }
        }

        private weak var tableView: LibraryNativeTableView?
        private var parent: LibraryTableView
        private var rowModels: [LibraryTableRow] = []
        private var rowsByID: [String: LibraryTableRow] = [:]
        private var rowIndexesByID: [String: Int] = [:]
        private var columnLabels: LibraryTableColumnLabels?
        private var selectedRowID: String?
        private var isSynchronizingSelection = false
        private var scheduledFocusRequestID: Int?
        private var fulfilledFocusRequestID: Int?

        init(parent: LibraryTableView) {
            self.parent = parent
        }

        func attach(_ tableView: LibraryNativeTableView) {
            self.tableView = tableView
        }

        func detach() {
            tableView?.delegate = nil
            tableView?.dataSource = nil
            tableView?.contextMenuProvider = nil
            tableView?.didMoveToWindow = nil
            tableView = nil
            rowModels = []
            rowsByID = [:]
            rowIndexesByID = [:]
        }

        func installColumns(in tableView: NSTableView) {
            let packageColumn = NSTableColumn(identifier: ColumnID.package)
            packageColumn.minWidth = 180
            packageColumn.width = 330
            packageColumn.resizingMask = .autoresizingMask
            tableView.addTableColumn(packageColumn)

            let managerColumn = NSTableColumn(identifier: ColumnID.manager)
            managerColumn.minWidth = 90
            managerColumn.width = 130
            managerColumn.resizingMask = .userResizingMask
            tableView.addTableColumn(managerColumn)

            let versionColumn = NSTableColumn(identifier: ColumnID.version)
            versionColumn.minWidth = 90
            versionColumn.width = 120
            versionColumn.resizingMask = .userResizingMask
            tableView.addTableColumn(versionColumn)

            let statusColumn = NSTableColumn(identifier: ColumnID.status)
            statusColumn.minWidth = 112
            statusColumn.width = 150
            statusColumn.resizingMask = .userResizingMask
            tableView.addTableColumn(statusColumn)
        }

        func update(parent: LibraryTableView) {
            self.parent = parent
            tableView?.setAccessibilityLabel(parent.accessibilityLabel)

            if columnLabels != parent.columnLabels {
                columnLabels = parent.columnLabels
                updateColumnLabels()
            }

            let modelChanged = rowModels != parent.rows
            if modelChanged {
                rowModels = parent.rows
                rowsByID = Dictionary(uniqueKeysWithValues: parent.rows.map { ($0.id, $0) })
                rowIndexesByID = Dictionary(
                    uniqueKeysWithValues: parent.rows.enumerated().map { ($0.element.id, $0.offset) }
                )
                tableView?.reloadData()
            }

            if selectedRowID != parent.selectedRowID || modelChanged {
                selectedRowID = parent.selectedRowID
                synchronizeSelection()
            }
            fulfillFocusRequestIfPossible()
        }

        func numberOfRows(in tableView: NSTableView) -> Int {
            rowModels.count
        }

        func tableView(
            _ tableView: NSTableView,
            viewFor tableColumn: NSTableColumn?,
            row: Int
        ) -> NSView? {
            guard rowModels.indices.contains(row), let columnID = tableColumn?.identifier else {
                return nil
            }
            let rowModel = rowModels[row]
            switch columnID {
            case ColumnID.package:
                return packageCell(in: tableView, for: rowModel)
            case ColumnID.manager:
                return textCell(
                    in: tableView,
                    identifier: ColumnID.manager,
                    value: rowModel.manager
                )
            case ColumnID.version:
                return versionCell(in: tableView, for: rowModel)
            case ColumnID.status:
                return statusCell(in: tableView, for: rowModel)
            default:
                return nil
            }
        }

        func tableViewSelectionDidChange(_ notification: Notification) {
            guard !isSynchronizingSelection, let tableView else { return }
            guard rowModels.indices.contains(tableView.selectedRow) else {
                selectedRowID = nil
                parent.onClearSelection()
                return
            }
            let row = rowModels[tableView.selectedRow]
            selectedRowID = row.id
            parent.onSelectRow(row)
        }

        func tableViewColumnDidResize(_ notification: Notification) {
            (tableView?.enclosingScrollView as? LibraryTableScrollView)?
                .fitDocumentWidthToViewport()
        }

        func tableView(
            _ tableView: NSTableView,
            typeSelectStringFor tableColumn: NSTableColumn?,
            row: Int
        ) -> String? {
            guard rowModels.indices.contains(row) else { return nil }
            return rowModels[row].name
        }

        fileprivate func contextMenu(forRowAt rowIndex: Int) -> NSMenu? {
            guard rowModels.indices.contains(rowIndex) else { return nil }
            let row = rowModels[rowIndex]
            let menu = NSMenu()

            let detailsItem = RowMenuItem(
                title: parent.columnLabels.viewDetails,
                action: #selector(showDetailsFromMenu(_:)),
                keyEquivalent: ""
            )
            detailsItem.command = LibraryTableCommand(
                rowID: row.id,
                packageID: row.selectedPackageID,
                actionIdentity: nil
            )
            detailsItem.target = self
            menu.addItem(detailsItem)

            if let action = row.action {
                menu.addItem(.separator())
                let actionItem = RowMenuItem(
                    title: action.title,
                    action: #selector(performActionFromMenu(_:)),
                    keyEquivalent: ""
                )
                actionItem.command = LibraryTableCommand(
                    rowID: row.id,
                    packageID: row.selectedPackageID,
                    actionIdentity: action.identity
                )
                actionItem.target = self
                actionItem.isEnabled = action.isEnabled && !action.isInFlight
                menu.addItem(actionItem)
            }
            return menu
        }

        fileprivate func fulfillFocusRequestIfPossible() {
            guard let tableView,
                  tableView.window != nil,
                  let request = parent.focusRequest,
                  request.requestID != fulfilledFocusRequestID,
                  request.requestID != scheduledFocusRequestID,
                  rowIndexesByID[request.rowID] != nil else {
                return
            }

            scheduledFocusRequestID = request.requestID
            DispatchQueue.main.async { [weak self, weak tableView] in
                guard let self, let tableView else { return }
                defer { self.scheduledFocusRequestID = nil }
                guard self.parent.focusRequest == request,
                      let rowIndex = self.rowIndexesByID[request.rowID],
                      tableView.window != nil else {
                    return
                }

                self.selectRow(at: rowIndex, in: tableView)
                tableView.scrollRowToVisible(rowIndex)
                guard tableView.window?.makeFirstResponder(tableView) == true else { return }
                self.fulfilledFocusRequestID = request.requestID
                self.parent.onFulfillFocusRequest(request.requestID)
            }
        }

        @objc private func actionPressed(_ sender: RowActionButton) {
            guard let command = sender.command,
                  let row = LibraryTableCommandPolicy.resolvedRow(
                      for: command,
                      in: rowsByID
                  ) else {
                return
            }
            parent.onSelectRow(row)
            parent.onPerformAction(row)
        }

        @objc private func showDetailsFromMenu(_ sender: RowMenuItem) {
            guard let command = sender.command,
                  let row = LibraryTableCommandPolicy.resolvedRow(
                      for: command,
                      in: rowsByID
                  ) else {
                return
            }
            parent.onSelectRow(row)
            parent.onShowDetails(row)
        }

        @objc private func performActionFromMenu(_ sender: RowMenuItem) {
            guard let command = sender.command,
                  let row = LibraryTableCommandPolicy.resolvedRow(
                      for: command,
                      in: rowsByID
                  ) else {
                return
            }
            parent.onSelectRow(row)
            parent.onPerformAction(row)
        }

        private func synchronizeSelection() {
            guard let tableView else { return }
            guard let selectedRowID,
                  let rowIndex = rowIndexesByID[selectedRowID] else {
                if tableView.selectedRow != -1 {
                    isSynchronizingSelection = true
                    tableView.deselectAll(nil)
                    isSynchronizingSelection = false
                }
                return
            }
            selectRow(at: rowIndex, in: tableView)
        }

        private func selectRow(at rowIndex: Int, in tableView: NSTableView) {
            guard tableView.selectedRow != rowIndex else { return }
            isSynchronizingSelection = true
            tableView.selectRowIndexes(IndexSet(integer: rowIndex), byExtendingSelection: false)
            isSynchronizingSelection = false
        }

        private func updateColumnLabels() {
            guard let tableView, let columnLabels else { return }
            tableView.tableColumn(withIdentifier: ColumnID.package)?.title = columnLabels.package
            tableView.tableColumn(withIdentifier: ColumnID.manager)?.title = columnLabels.manager
            tableView.tableColumn(withIdentifier: ColumnID.version)?.title = columnLabels.version
            tableView.tableColumn(withIdentifier: ColumnID.status)?.title = columnLabels.status
        }

        private func packageCell(
            in tableView: NSTableView,
            for row: LibraryTableRow
        ) -> NSView {
            let identifier = NSUserInterfaceItemIdentifier("library.package.cell")
            let cell = tableView.makeView(withIdentifier: identifier, owner: nil) as? PackageCell
                ?? PackageCell(frame: .zero)
            cell.identifier = identifier
            cell.titleLabel.stringValue = row.name
            cell.titleLabel.toolTip = row.name
            cell.detailLabel.stringValue = row.detail ?? ""
            cell.detailLabel.isHidden = row.detail?.isEmpty != false
            cell.detailLabel.toolTip = row.detail
            cell.pinImage.isHidden = !row.isPinned
            cell.pinImage.toolTip = row.isPinned ? parent.columnLabels.pinned : nil
            cell.statusImage.image = NSImage(
                systemSymbolName: row.statusSymbolName,
                accessibilityDescription: ""
            )
            cell.statusImage.contentTintColor = row.statusTone.color
            cell.toolTip = row.detail
            cell.semanticAccessibilityLabel = LibraryTableAccessibilityPolicy.packageDescription(
                for: row,
                labels: parent.columnLabels
            )
            cell.semanticAccessibilityChildren = []
            return cell
        }

        private func textCell(
            in tableView: NSTableView,
            identifier: NSUserInterfaceItemIdentifier,
            value: String
        ) -> NSView {
            let cell = tableView.makeView(
                withIdentifier: identifier,
                owner: nil
            ) as? SemanticTableCellView ?? SemanticTableCellView(frame: .zero)
            cell.identifier = identifier
            let label: NSTextField
            if let existing = cell.textField {
                label = existing
            } else {
                label = NSTextField(labelWithString: "")
                label.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
                label.textColor = .secondaryLabelColor
                label.lineBreakMode = .byTruncatingTail
                label.maximumNumberOfLines = 1
                label.translatesAutoresizingMaskIntoConstraints = false
                cell.addSubview(label)
                NSLayoutConstraint.activate([
                    label.leadingAnchor.constraint(equalTo: cell.leadingAnchor, constant: 4),
                    label.trailingAnchor.constraint(lessThanOrEqualTo: cell.trailingAnchor, constant: -4),
                    label.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
                ])
                cell.textField = label
            }
            label.stringValue = value
            label.toolTip = value
            cell.semanticAccessibilityLabel = value
            cell.semanticAccessibilityChildren = []
            return cell
        }

        private func versionCell(
            in tableView: NSTableView,
            for row: LibraryTableRow
        ) -> NSView {
            let identifier = NSUserInterfaceItemIdentifier("library.version.cell")
            let cell = tableView.makeView(withIdentifier: identifier, owner: nil) as? VersionCell
                ?? VersionCell(frame: .zero)
            cell.identifier = identifier

            if let latestVersion = row.latestVersion {
                cell.primaryLabel.stringValue = latestVersion
                cell.primaryLabel.textColor = .systemTeal
                cell.secondaryLabel.attributedStringValue = NSAttributedString(
                    string: row.currentVersion,
                    attributes: [
                        .foregroundColor: NSColor.secondaryLabelColor,
                        .strikethroughStyle: NSUnderlineStyle.single.rawValue,
                    ]
                )
                cell.secondaryLabel.isHidden = false
            } else {
                cell.primaryLabel.stringValue = row.currentVersion
                cell.primaryLabel.textColor = .labelColor
                cell.secondaryLabel.stringValue = ""
                cell.secondaryLabel.isHidden = true
            }
            cell.primaryLabel.toolTip = row.latestVersion ?? row.currentVersion
            cell.secondaryLabel.toolTip = row.currentVersion
            cell.restartImage.isHidden = !row.isRestartRequired
            cell.restartImage.toolTip = row.isRestartRequired
                ? parent.columnLabels.restartRequired
                : nil
            cell.semanticAccessibilityLabel = LibraryTableAccessibilityPolicy.versionDescription(
                for: row,
                labels: parent.columnLabels
            )
            cell.semanticAccessibilityChildren = []
            return cell
        }

        private func statusCell(
            in tableView: NSTableView,
            for row: LibraryTableRow
        ) -> NSView {
            let identifier = NSUserInterfaceItemIdentifier("library.status.cell")
            let cell = tableView.makeView(withIdentifier: identifier, owner: nil) as? StatusCell
                ?? StatusCell(frame: .zero)
            cell.identifier = identifier
            cell.statusLabel.stringValue = row.status
            cell.statusLabel.textColor = row.statusTone.color
            cell.statusLabel.toolTip = row.status

            let action = row.action
            cell.actionButton.command = action.map {
                LibraryTableCommand(
                    rowID: row.id,
                    packageID: row.selectedPackageID,
                    actionIdentity: $0.identity
                )
            }
            cell.actionButton.target = self
            cell.actionButton.action = #selector(actionPressed(_:))
            cell.actionButton.image = action.flatMap {
                NSImage(systemSymbolName: $0.symbolName, accessibilityDescription: $0.title)
            }
            cell.actionButton.toolTip = action?.title
            cell.actionButton.setAccessibilityLabel(action?.title)
            cell.actionButton.isEnabled = action?.isEnabled == true && action?.isInFlight != true
            cell.actionButton.isHidden = action == nil || action?.isInFlight == true

            cell.progressIndicator.isHidden = action?.isInFlight != true
            if action?.isInFlight == true {
                cell.progressIndicator.startAnimation(nil)
            } else {
                cell.progressIndicator.stopAnimation(nil)
            }
            cell.semanticAccessibilityLabel = action?.isInFlight == true
                ? "\(row.status), \(parent.columnLabels.running)"
                : row.status
            cell.semanticAccessibilityChildren = cell.actionButton.isHidden
                ? []
                : cell.actionButton.cell.map { [$0] } ?? []
            return cell
        }
    }
}

private extension LibraryTableStatusTone {
    var color: NSColor {
        switch self {
        case .healthy:
            return .systemGreen
        case .updatesReady:
            return .systemTeal
        case .available:
            return .controlAccentColor
        case .neutral:
            return .secondaryLabelColor
        }
    }
}

final class LibraryTableScrollView: NSScrollView {
    override func tile() {
        super.tile()
        fitDocumentWidthToViewport()
    }

    func fitDocumentWidthToViewport() {
        guard let tableView = documentView as? LibraryNativeTableView else { return }
        let viewportWidth = contentView.bounds.width
        tableView.fitLeadingColumn(to: viewportWidth)
        let width = max(viewportWidth, tableView.requiredContentWidth)
        guard width > 0, abs(tableView.frame.width - width) > 0.5 else {
            return
        }
        tableView.setFrameSize(NSSize(width: width, height: tableView.frame.height))
    }
}

final class LibraryNativeTableView: NSTableView {
    var contextMenuProvider: ((Int) -> NSMenu?)?
    var didMoveToWindow: (() -> Void)?

    var minimumContentWidth: CGFloat {
        guard let leadingColumn = tableColumns.first else { return 0 }
        let fixedWidth = tableColumns.dropFirst().reduce(CGFloat.zero) { $0 + $1.width }
        return leadingColumn.minWidth + fixedWidth
            + LibraryTableLayoutPolicy.layoutOverhead(in: self)
    }

    var requiredContentWidth: CGFloat {
        LibraryTableLayoutPolicy.requiredContentWidth(in: self)
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        didMoveToWindow?()
    }

    override func layout() {
        let viewportWidth = enclosingScrollView?.contentView.bounds.width ?? bounds.width
        fitLeadingColumn(to: viewportWidth)
        super.layout()
    }

    override func menu(for event: NSEvent) -> NSMenu? {
        let location = convert(event.locationInWindow, from: nil)
        let rowIndex = row(at: location)
        guard rowIndex >= 0 else { return nil }
        if selectedRow != rowIndex {
            selectRowIndexes(IndexSet(integer: rowIndex), byExtendingSelection: false)
        }
        return contextMenuProvider?(rowIndex)
    }

    func fitLeadingColumn(to viewportWidth: CGFloat) {
        LibraryTableLayoutPolicy.fitLeadingColumn(in: self, to: viewportWidth)
    }
}
