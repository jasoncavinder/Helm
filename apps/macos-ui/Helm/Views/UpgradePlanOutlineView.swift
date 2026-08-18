import AppKit
import SwiftUI

struct UpgradePlanOutlineSection: Equatable, Identifiable {
    let id: String
    let title: String
    let summary: String
    let rows: [UpgradePlanOutlineRow]
}

struct UpgradePlanOutlineRow: Equatable, Identifiable {
    enum StatusTone: Equatable {
        case standard
        case needsReview
        case error
    }

    let id: String
    let sequence: Int
    let title: String
    let manager: String
    let isIncluded: Bool
    let isSelectable: Bool
    let status: String
    let statusTone: StatusTone
    let actionTitle: String?
}

struct UpgradePlanOutlineColumnLabels: Equatable {
    let update: String
    let manager: String
    let included: String
    let status: String
    let action: String
}

struct UpgradePlanOutlineView: NSViewRepresentable {
    let sections: [UpgradePlanOutlineSection]
    let selectedStepID: String?
    let columnLabels: UpgradePlanOutlineColumnLabels
    let accessibilityLabel: String
    let interactionsEnabled: Bool
    let onSelectStep: (String) -> Void
    let onSetIncluded: (String, Bool) -> Void
    let onPerformAction: (String) -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(parent: self)
    }

    func makeNSView(context: Context) -> NSScrollView {
        let outlineView = UpgradePlanNativeOutlineView()
        outlineView.dataSource = context.coordinator
        outlineView.delegate = context.coordinator
        outlineView.allowsEmptySelection = true
        outlineView.allowsMultipleSelection = false
        outlineView.autosaveExpandedItems = false
        outlineView.backgroundColor = .clear
        outlineView.columnAutoresizingStyle = .firstColumnOnlyAutoresizingStyle
        outlineView.floatsGroupRows = false
        outlineView.style = .plain
        outlineView.headerView = NSTableHeaderView()
        outlineView.indentationPerLevel = 10
        outlineView.intercellSpacing = NSSize(width: 6, height: 2)
        outlineView.rowSizeStyle = .medium
        outlineView.selectionHighlightStyle = .regular
        outlineView.gridStyleMask = .solidHorizontalGridLineMask
        outlineView.gridColor = .separatorColor.withAlphaComponent(0.35)
        outlineView.setAccessibilityLabel(accessibilityLabel)
        outlineView.toggleCurrentRow = { [weak coordinator = context.coordinator] in
            coordinator?.toggleCurrentRow() ?? false
        }

        context.coordinator.installColumns(in: outlineView)

        let scrollView = NSScrollView()
        scrollView.autohidesScrollers = true
        scrollView.borderType = .noBorder
        scrollView.documentView = outlineView
        scrollView.drawsBackground = false
        scrollView.scrollerStyle = .overlay
        scrollView.hasHorizontalScroller = false
        scrollView.hasVerticalScroller = true

        context.coordinator.attach(outlineView)
        context.coordinator.update(parent: self)
        return scrollView
    }

    func updateNSView(_ scrollView: NSScrollView, context: Context) {
        context.coordinator.update(parent: self)
    }

    static func dismantleNSView(_ scrollView: NSScrollView, coordinator: Coordinator) {
        coordinator.detach()
    }

    final class Coordinator: NSObject, NSOutlineViewDataSource, NSOutlineViewDelegate {
        private enum ColumnID {
            static let update = NSUserInterfaceItemIdentifier("plan.update")
            static let manager = NSUserInterfaceItemIdentifier("plan.manager")
            static let included = NSUserInterfaceItemIdentifier("plan.included")
            static let status = NSUserInterfaceItemIdentifier("plan.status")
            static let action = NSUserInterfaceItemIdentifier("plan.action")
        }

        private final class SectionNode: NSObject {
            let id: String
            var title: String
            var summary: String
            var children: [RowNode]

            init(section: UpgradePlanOutlineSection) {
                id = section.id
                title = section.title
                summary = section.summary
                children = section.rows.map(RowNode.init)
            }
        }

        private final class RowNode: NSObject {
            var row: UpgradePlanOutlineRow

            init(row: UpgradePlanOutlineRow) {
                self.row = row
            }
        }

        private final class StepCheckbox: NSButton {
            var stepID = ""
        }

        private final class StepActionButton: NSButton {
            var stepID = ""
        }

        private final class StepSummaryCell: NSTableCellView {
            let titleLabel = NSTextField(labelWithString: "")
            let managerLabel = NSTextField(labelWithString: "")

            override init(frame frameRect: NSRect) {
                super.init(frame: frameRect)

                titleLabel.font = .systemFont(ofSize: NSFont.systemFontSize)
                titleLabel.lineBreakMode = .byTruncatingTail
                titleLabel.maximumNumberOfLines = 1
                managerLabel.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
                managerLabel.textColor = .secondaryLabelColor
                managerLabel.lineBreakMode = .byTruncatingTail
                managerLabel.maximumNumberOfLines = 1

                let stack = NSStackView(views: [titleLabel, managerLabel])
                stack.orientation = .vertical
                stack.alignment = .leading
                stack.spacing = 1
                stack.translatesAutoresizingMaskIntoConstraints = false
                addSubview(stack)
                NSLayoutConstraint.activate([
                    stack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 2),
                    stack.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -4),
                    stack.centerYAnchor.constraint(equalTo: centerYAnchor),
                ])
                textField = titleLabel
            }

            @available(*, unavailable)
            required init?(coder: NSCoder) {
                fatalError("init(coder:) has not been implemented")
            }
        }

        private weak var outlineView: UpgradePlanNativeOutlineView?
        private var parent: UpgradePlanOutlineView
        private var sectionModels: [UpgradePlanOutlineSection] = []
        private var sectionNodes: [SectionNode] = []
        private var rowNodesByID: [String: RowNode] = [:]
        private var selectedStepID: String?
        private var columnLabels: UpgradePlanOutlineColumnLabels?
        private var interactionsEnabled = true
        private var isSynchronizingSelection = false

        init(parent: UpgradePlanOutlineView) {
            self.parent = parent
        }

        fileprivate func attach(_ outlineView: UpgradePlanNativeOutlineView) {
            self.outlineView = outlineView
        }

        func detach() {
            outlineView?.delegate = nil
            outlineView?.dataSource = nil
            outlineView = nil
            sectionNodes = []
            rowNodesByID = [:]
        }

        func installColumns(in outlineView: NSOutlineView) {
            let updateColumn = NSTableColumn(identifier: ColumnID.update)
            updateColumn.minWidth = 220
            updateColumn.width = 360
            updateColumn.resizingMask = [.autoresizingMask, .userResizingMask]
            outlineView.addTableColumn(updateColumn)
            outlineView.outlineTableColumn = updateColumn

            let includedColumn = NSTableColumn(identifier: ColumnID.included)
            includedColumn.minWidth = 62
            includedColumn.maxWidth = 78
            includedColumn.width = 68
            includedColumn.resizingMask = .userResizingMask
            outlineView.addTableColumn(includedColumn)

            let statusColumn = NSTableColumn(identifier: ColumnID.status)
            statusColumn.minWidth = 96
            statusColumn.width = 120
            statusColumn.resizingMask = .userResizingMask
            outlineView.addTableColumn(statusColumn)

            let actionColumn = NSTableColumn(identifier: ColumnID.action)
            actionColumn.minWidth = 110
            actionColumn.width = 136
            actionColumn.resizingMask = .userResizingMask
            outlineView.addTableColumn(actionColumn)
        }

        func update(parent: UpgradePlanOutlineView) {
            self.parent = parent
            outlineView?.setAccessibilityLabel(parent.accessibilityLabel)

            if columnLabels != parent.columnLabels {
                columnLabels = parent.columnLabels
                updateColumnLabels()
            }

            let modelChanged = sectionModels != parent.sections
                || interactionsEnabled != parent.interactionsEnabled
            if modelChanged {
                rebuildNodes(
                    from: parent.sections,
                    interactionsEnabled: parent.interactionsEnabled
                )
            }

            if selectedStepID != parent.selectedStepID || modelChanged {
                selectedStepID = parent.selectedStepID
                synchronizeSelection()
            }
        }

        func outlineView(
            _ outlineView: NSOutlineView,
            numberOfChildrenOfItem item: Any?
        ) -> Int {
            if item == nil {
                return sectionNodes.count
            }
            return (item as? SectionNode)?.children.count ?? 0
        }

        func outlineView(
            _ outlineView: NSOutlineView,
            child index: Int,
            ofItem item: Any?
        ) -> Any {
            if let section = item as? SectionNode {
                return section.children[index]
            }
            return sectionNodes[index]
        }

        func outlineView(_ outlineView: NSOutlineView, isItemExpandable item: Any) -> Bool {
            item is SectionNode
        }

        func outlineView(_ outlineView: NSOutlineView, isGroupItem item: Any) -> Bool {
            item is SectionNode
        }

        func outlineView(_ outlineView: NSOutlineView, shouldSelectItem item: Any) -> Bool {
            item is RowNode
        }

        func outlineView(_ outlineView: NSOutlineView, heightOfRowByItem item: Any) -> CGFloat {
            item is SectionNode ? 30 : 46
        }

        func outlineView(_ outlineView: NSOutlineView, rowViewForItem item: Any) -> NSTableRowView? {
            UpgradePlanOutlineRowView()
        }

        func outlineView(
            _ outlineView: NSOutlineView,
            viewFor tableColumn: NSTableColumn?,
            item: Any
        ) -> NSView? {
            if let section = item as? SectionNode {
                guard tableColumn == nil || tableColumn?.identifier == ColumnID.update else {
                    return nil
                }
                return textCell(
                    in: outlineView,
                    identifier: NSUserInterfaceItemIdentifier("plan.group"),
                    value: "\(section.title) (\(section.summary))",
                    font: .systemFont(ofSize: NSFont.systemFontSize, weight: .semibold),
                    color: .labelColor
                )
            }

            guard let columnID = tableColumn?.identifier else { return nil }
            guard let rowNode = item as? RowNode else { return nil }
            let row = rowNode.row
            switch columnID {
            case ColumnID.update:
                return summaryCell(in: outlineView, for: row)
            case ColumnID.manager:
                return textCell(
                    in: outlineView,
                    identifier: ColumnID.manager,
                    value: row.manager,
                    font: .systemFont(ofSize: NSFont.smallSystemFontSize),
                    color: .secondaryLabelColor
                )
            case ColumnID.included:
                return checkbox(in: outlineView, for: row)
            case ColumnID.status:
                let field = textCell(
                    in: outlineView,
                    identifier: ColumnID.status,
                    value: row.status,
                    font: .systemFont(ofSize: NSFont.smallSystemFontSize),
                    color: statusColor(for: row.statusTone)
                )
                field.alignment = .right
                return field
            case ColumnID.action:
                return actionButton(in: outlineView, for: row)
            default:
                return nil
            }
        }

        func outlineViewSelectionDidChange(_ notification: Notification) {
            guard !isSynchronizingSelection,
                  let outlineView,
                  outlineView.selectedRow >= 0,
                  let rowNode = outlineView.item(atRow: outlineView.selectedRow) as? RowNode else {
                return
            }
            selectedStepID = rowNode.row.id
            parent.onSelectStep(rowNode.row.id)
        }

        func toggleCurrentRow() -> Bool {
            guard interactionsEnabled,
                  let outlineView,
                  outlineView.selectedRow >= 0,
                  let rowNode = outlineView.item(atRow: outlineView.selectedRow) as? RowNode,
                  rowNode.row.isSelectable else {
                return false
            }
            setIncluded(!rowNode.row.isIncluded, for: rowNode)
            return true
        }

        @objc private func checkboxChanged(_ sender: StepCheckbox) {
            guard interactionsEnabled,
                  let rowNode = rowNodesByID[sender.stepID],
                  rowNode.row.isSelectable else {
                return
            }
            setIncluded(sender.state == .on, for: rowNode)
        }

        @objc private func actionPressed(_ sender: StepActionButton) {
            parent.onSelectStep(sender.stepID)
            parent.onPerformAction(sender.stepID)
        }

        private func rebuildNodes(
            from sections: [UpgradePlanOutlineSection],
            interactionsEnabled: Bool
        ) {
            guard let outlineView else { return }
            let hadModel = !sectionModels.isEmpty
            let expandedIDs = Set(
                sectionNodes
                    .filter { outlineView.isItemExpanded($0) }
                    .map(\.id)
            )

            sectionModels = sections
            self.interactionsEnabled = interactionsEnabled
            sectionNodes = sections.map(SectionNode.init)
            rowNodesByID = Dictionary(
                uniqueKeysWithValues: sectionNodes.flatMap { section in
                    section.children.map { ($0.row.id, $0) }
                }
            )

            outlineView.reloadData()
            for section in sectionNodes where !hadModel || expandedIDs.contains(section.id) {
                outlineView.expandItem(section)
            }
        }

        private func synchronizeSelection() {
            guard let outlineView else { return }
            isSynchronizingSelection = true
            defer { isSynchronizingSelection = false }

            guard let selectedStepID,
                  let rowNode = rowNodesByID[selectedStepID],
                  let section = sectionNodes.first(where: { $0.children.contains(where: { $0 === rowNode }) }) else {
                outlineView.deselectAll(nil)
                return
            }

            if !outlineView.isItemExpanded(section) {
                outlineView.expandItem(section)
            }
            let rowIndex = outlineView.row(forItem: rowNode)
            guard rowIndex >= 0 else {
                outlineView.deselectAll(nil)
                return
            }
            if outlineView.selectedRow != rowIndex {
                outlineView.selectRowIndexes(IndexSet(integer: rowIndex), byExtendingSelection: false)
            }
        }

        private func setIncluded(_ included: Bool, for rowNode: RowNode) {
            rowNode.row = UpgradePlanOutlineRow(
                id: rowNode.row.id,
                sequence: rowNode.row.sequence,
                title: rowNode.row.title,
                manager: rowNode.row.manager,
                isIncluded: included,
                isSelectable: rowNode.row.isSelectable,
                status: rowNode.row.status,
                statusTone: rowNode.row.statusTone,
                actionTitle: rowNode.row.actionTitle
            )
            parent.onSetIncluded(rowNode.row.id, included)
        }

        private func updateColumnLabels() {
            guard let outlineView, let labels = columnLabels else { return }
            outlineView.tableColumn(withIdentifier: ColumnID.update)?.title = labels.update
            outlineView.tableColumn(withIdentifier: ColumnID.manager)?.title = labels.manager
            outlineView.tableColumn(withIdentifier: ColumnID.included)?.title = labels.included
            outlineView.tableColumn(withIdentifier: ColumnID.status)?.title = labels.status
            outlineView.tableColumn(withIdentifier: ColumnID.action)?.title = labels.action
        }

        private func textCell(
            in outlineView: NSOutlineView,
            identifier: NSUserInterfaceItemIdentifier,
            value: String,
            font: NSFont,
            color: NSColor
        ) -> NSTextField {
            let field = outlineView.makeView(withIdentifier: identifier, owner: nil) as? NSTextField
                ?? NSTextField(labelWithString: "")
            field.identifier = identifier
            field.font = font
            field.lineBreakMode = .byTruncatingTail
            field.maximumNumberOfLines = 1
            field.stringValue = value
            field.textColor = color
            field.toolTip = value
            return field
        }

        private func summaryCell(
            in outlineView: NSOutlineView,
            for row: UpgradePlanOutlineRow
        ) -> StepSummaryCell {
            let identifier = ColumnID.update
            let cell = outlineView.makeView(withIdentifier: identifier, owner: nil) as? StepSummaryCell
                ?? StepSummaryCell(frame: .zero)
            let title = "\(row.sequence). \(row.title)"
            cell.identifier = identifier
            cell.titleLabel.stringValue = title
            cell.titleLabel.toolTip = title
            cell.managerLabel.stringValue = row.manager
            cell.managerLabel.toolTip = row.manager
            cell.setAccessibilityLabel("\(title), \(columnLabels?.manager ?? ""), \(row.manager)")
            return cell
        }

        private func checkbox(
            in outlineView: NSOutlineView,
            for row: UpgradePlanOutlineRow
        ) -> StepCheckbox {
            let identifier = ColumnID.included
            let button = outlineView.makeView(withIdentifier: identifier, owner: nil) as? StepCheckbox
                ?? StepCheckbox(checkboxWithTitle: "", target: self, action: #selector(checkboxChanged(_:)))
            button.identifier = identifier
            button.target = self
            button.action = #selector(checkboxChanged(_:))
            button.stepID = row.id
            button.state = row.isIncluded ? .on : .off
            button.isEnabled = interactionsEnabled && row.isSelectable
            button.setAccessibilityLabel("\(columnLabels?.included ?? ""), \(row.title)")
            return button
        }

        private func actionButton(
            in outlineView: NSOutlineView,
            for row: UpgradePlanOutlineRow
        ) -> NSButton? {
            guard let actionTitle = row.actionTitle else { return nil }
            let identifier = ColumnID.action
            let button = outlineView.makeView(withIdentifier: identifier, owner: nil) as? StepActionButton
                ?? StepActionButton(title: "", target: self, action: #selector(actionPressed(_:)))
            button.identifier = identifier
            button.target = self
            button.action = #selector(actionPressed(_:))
            button.bezelStyle = .rounded
            button.controlSize = .small
            button.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
            button.lineBreakMode = .byTruncatingTail
            button.stepID = row.id
            button.title = actionTitle
            button.toolTip = actionTitle
            button.isEnabled = interactionsEnabled
            return button
        }

        private func statusColor(for tone: UpgradePlanOutlineRow.StatusTone) -> NSColor {
            switch tone {
            case .standard:
                return .secondaryLabelColor
            case .needsReview:
                return NSColor(HelmTheme.stateNeedsReview)
            case .error:
                return .systemRed
            }
        }
    }
}

private final class UpgradePlanOutlineRowView: NSTableRowView {
    override func drawSelection(in dirtyRect: NSRect) {
        guard selectionHighlightStyle != .none else { return }
        NSColor(HelmTheme.selectionFill).setFill()
        NSBezierPath(
            roundedRect: bounds.insetBy(dx: 2, dy: 1),
            xRadius: 7,
            yRadius: 7
        ).fill()
    }
}

private final class UpgradePlanNativeOutlineView: NSOutlineView {
    var toggleCurrentRow: (() -> Bool)?

    override func keyDown(with event: NSEvent) {
        let blockedModifiers: NSEvent.ModifierFlags = [.command, .control, .option]
        if event.charactersIgnoringModifiers == " ",
           event.modifierFlags.isDisjoint(with: blockedModifiers),
           toggleCurrentRow?() == true {
            return
        }
        super.keyDown(with: event)
    }
}
