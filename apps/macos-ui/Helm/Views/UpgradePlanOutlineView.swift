import AppKit
import SwiftUI

private enum UpgradePlanOutlineMetrics {
    static let hierarchyIndent: CGFloat = 10
    static let disclosureVerticalOffset: CGFloat = 1
    static let cardLeadingInset: CGFloat = 26
    static let cardTrailingInset: CGFloat = 4
    static let cardVerticalInset: CGFloat = 3
    static let leadingContentInset: CGFloat = 6
    static let trailingContentInset: CGFloat = 18
}

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
        outlineView.autoresizingMask = [.width]
        outlineView.columnAutoresizingStyle = .noColumnAutoresizing
        outlineView.floatsGroupRows = false
        outlineView.style = .plain
        outlineView.headerView = nil
        outlineView.indentationPerLevel = UpgradePlanOutlineMetrics.hierarchyIndent
        outlineView.intercellSpacing = NSSize(width: 6, height: 6)
        outlineView.rowSizeStyle = .medium
        outlineView.selectionHighlightStyle = .regular
        outlineView.gridStyleMask = []
        outlineView.setAccessibilityLabel(accessibilityLabel)
        outlineView.toggleCurrentRow = { [weak coordinator = context.coordinator] in
            coordinator?.toggleCurrentRow() ?? false
        }

        context.coordinator.installColumns(in: outlineView)

        let scrollView = UpgradePlanScrollView()
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
        (scrollView as? UpgradePlanScrollView)?.fitDocumentWidthToViewport()
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
            let inclusionCheckbox = StepCheckbox(
                checkboxWithTitle: "",
                target: nil,
                action: nil
            )
            let titleLabel = NSTextField(labelWithString: "")
            let managerLabel = NSTextField(labelWithString: "")

            override init(frame frameRect: NSRect) {
                super.init(frame: frameRect)

                titleLabel.font = .systemFont(ofSize: NSFont.systemFontSize)
                titleLabel.textColor = .labelColor
                titleLabel.lineBreakMode = .byTruncatingTail
                titleLabel.maximumNumberOfLines = 1
                managerLabel.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
                managerLabel.textColor = .secondaryLabelColor
                managerLabel.lineBreakMode = .byTruncatingTail
                managerLabel.maximumNumberOfLines = 1

                inclusionCheckbox.controlSize = .small
                inclusionCheckbox.setContentHuggingPriority(.required, for: .horizontal)

                let textStack = NSStackView(views: [titleLabel, managerLabel])
                textStack.orientation = .vertical
                textStack.alignment = .leading
                textStack.spacing = 1

                let contentStack = NSStackView(views: [inclusionCheckbox, textStack])
                contentStack.orientation = .horizontal
                contentStack.alignment = .centerY
                contentStack.spacing = 12
                contentStack.translatesAutoresizingMaskIntoConstraints = false
                addSubview(contentStack)
                NSLayoutConstraint.activate([
                    contentStack.leadingAnchor.constraint(
                        equalTo: leadingAnchor,
                        constant: UpgradePlanOutlineMetrics.leadingContentInset
                    ),
                    contentStack.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -6),
                    contentStack.centerYAnchor.constraint(equalTo: centerYAnchor),
                ])
                textField = titleLabel
            }

            @available(*, unavailable)
            required init?(coder: NSCoder) {
                fatalError("init(coder:) has not been implemented")
            }
        }

        private final class StepTrailingCell: NSTableCellView {
            let statusLabel = NSTextField(labelWithString: "")
            let actionButton = StepActionButton(title: "", target: nil, action: nil)

            override init(frame frameRect: NSRect) {
                super.init(frame: frameRect)

                statusLabel.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
                statusLabel.alignment = .right
                statusLabel.lineBreakMode = .byTruncatingTail
                statusLabel.maximumNumberOfLines = 1
                statusLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

                actionButton.bezelStyle = .rounded
                actionButton.controlSize = .small
                actionButton.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
                actionButton.lineBreakMode = .byTruncatingTail
                actionButton.setContentCompressionResistancePriority(.required, for: .horizontal)

                let stack = NSStackView(views: [statusLabel, actionButton])
                stack.orientation = .horizontal
                stack.alignment = .centerY
                stack.spacing = 8
                stack.translatesAutoresizingMaskIntoConstraints = false
                addSubview(stack)
                NSLayoutConstraint.activate([
                    stack.leadingAnchor.constraint(greaterThanOrEqualTo: leadingAnchor, constant: 4),
                    stack.trailingAnchor.constraint(
                        equalTo: trailingAnchor,
                        constant: -UpgradePlanOutlineMetrics.trailingContentInset
                    ),
                    stack.centerYAnchor.constraint(equalTo: centerYAnchor),
                ])
                textField = statusLabel
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
        private var expansionState = UpgradePlanSectionExpansionState()

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
            updateColumn.minWidth = 180
            updateColumn.width = 340
            updateColumn.resizingMask = [.autoresizingMask, .userResizingMask]
            outlineView.addTableColumn(updateColumn)
            outlineView.outlineTableColumn = updateColumn

            let statusColumn = NSTableColumn(identifier: ColumnID.status)
            statusColumn.minWidth = 132
            statusColumn.width = 180
            statusColumn.resizingMask = .userResizingMask
            outlineView.addTableColumn(statusColumn)
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
            item is SectionNode ? 28 : 50
        }

        func outlineView(_ outlineView: NSOutlineView, rowViewForItem item: Any) -> NSTableRowView? {
            UpgradePlanOutlineRowView(drawsCard: item is RowNode)
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
                    font: .systemFont(ofSize: NSFont.smallSystemFontSize, weight: .semibold),
                    color: .secondaryLabelColor
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
            case ColumnID.status:
                return trailingCell(in: outlineView, for: row)
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
            expansionState.recordVisibleSections(
                Set(sectionNodes.map(\.id)),
                expandedSectionIDs: Set(
                    sectionNodes
                        .filter { outlineView.isItemExpanded($0) }
                        .map(\.id)
                )
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
            let sectionsToExpand = expansionState.sectionsToExpand(
                from: Set(sectionNodes.map(\.id))
            )
            for section in sectionNodes where sectionsToExpand.contains(section.id) {
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
            outlineView.tableColumn(withIdentifier: ColumnID.status)?.title = labels.status
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
            cell.inclusionCheckbox.identifier = ColumnID.included
            cell.inclusionCheckbox.target = self
            cell.inclusionCheckbox.action = #selector(checkboxChanged(_:))
            cell.inclusionCheckbox.stepID = row.id
            cell.inclusionCheckbox.state = row.isIncluded ? .on : .off
            cell.inclusionCheckbox.isEnabled = interactionsEnabled && row.isSelectable
            cell.inclusionCheckbox.setAccessibilityLabel(
                "\(columnLabels?.included ?? ""), \(row.title)"
            )
            cell.setAccessibilityLabel("\(title), \(columnLabels?.manager ?? ""), \(row.manager)")
            return cell
        }

        private func trailingCell(
            in outlineView: NSOutlineView,
            for row: UpgradePlanOutlineRow
        ) -> StepTrailingCell {
            let identifier = ColumnID.status
            let cell = outlineView.makeView(withIdentifier: identifier, owner: nil) as? StepTrailingCell
                ?? StepTrailingCell(frame: .zero)
            cell.identifier = identifier
            cell.statusLabel.stringValue = row.status
            cell.statusLabel.textColor = statusColor(for: row.statusTone)
            cell.statusLabel.toolTip = row.status
            if let actionTitle = row.actionTitle {
                cell.actionButton.identifier = ColumnID.action
                cell.actionButton.target = self
                cell.actionButton.action = #selector(actionPressed(_:))
                cell.actionButton.stepID = row.id
                cell.actionButton.title = actionTitle
                cell.actionButton.toolTip = actionTitle
                cell.actionButton.isEnabled = interactionsEnabled
                cell.actionButton.isHidden = false
                cell.actionButton.setAccessibilityLabel(actionTitle)
            } else {
                cell.actionButton.target = nil
                cell.actionButton.action = nil
                cell.actionButton.stepID = ""
                cell.actionButton.title = ""
                cell.actionButton.toolTip = nil
                cell.actionButton.isEnabled = false
                cell.actionButton.isHidden = true
            }
            cell.setAccessibilityLabel("\(columnLabels?.status ?? ""), \(row.status)")
            return cell
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
    private let drawsCard: Bool

    init(drawsCard: Bool) {
        self.drawsCard = drawsCard
        super.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func drawBackground(in dirtyRect: NSRect) {
        guard drawsCard else { return }
        let path = cardPath
        NSColor(HelmTheme.surfaceElevated).setFill()
        path.fill()
        NSColor(HelmTheme.borderSubtle).withAlphaComponent(0.9).setStroke()
        path.lineWidth = 0.8
        path.stroke()
    }

    override func drawSelection(in dirtyRect: NSRect) {
        guard drawsCard, selectionHighlightStyle != .none else { return }
        let path = cardPath
        NSColor(HelmTheme.selectionFill).setFill()
        path.fill()
        NSColor(HelmTheme.selectionStroke).setStroke()
        path.lineWidth = 0.9
        path.stroke()
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        needsDisplay = true
    }

    private var cardPath: NSBezierPath {
        let cardRect = NSRect(
            x: bounds.minX + UpgradePlanOutlineMetrics.cardLeadingInset,
            y: bounds.minY + UpgradePlanOutlineMetrics.cardVerticalInset,
            width: max(
                0,
                bounds.width
                    - UpgradePlanOutlineMetrics.cardLeadingInset
                    - UpgradePlanOutlineMetrics.cardTrailingInset
            ),
            height: max(0, bounds.height - (UpgradePlanOutlineMetrics.cardVerticalInset * 2))
        )
        return NSBezierPath(
            roundedRect: cardRect,
            xRadius: 8,
            yRadius: 8
        )
    }
}

private final class UpgradePlanScrollView: NSScrollView {
    override func tile() {
        super.tile()
        fitDocumentWidthToViewport()
    }

    func fitDocumentWidthToViewport() {
        guard let documentView else { return }
        let viewportWidth = contentView.bounds.width
        guard viewportWidth > 0, abs(documentView.frame.width - viewportWidth) > 0.5 else {
            return
        }
        documentView.setFrameSize(
            NSSize(width: viewportWidth, height: documentView.frame.height)
        )
        (documentView as? UpgradePlanNativeOutlineView)?.fitColumnsToBounds()
    }
}

private final class UpgradePlanNativeOutlineView: NSOutlineView {
    var toggleCurrentRow: (() -> Bool)?

    override func frameOfOutlineCell(atRow row: Int) -> NSRect {
        super.frameOfOutlineCell(atRow: row).offsetBy(
            dx: 0,
            dy: UpgradePlanOutlineMetrics.disclosureVerticalOffset
        )
    }

    override func layout() {
        fitColumnsToBounds()
        super.layout()
    }

    func fitColumnsToBounds() {
        guard bounds.width > 0, let leadingColumn = tableColumns.first else { return }
        let fixedColumnWidth = tableColumns.dropFirst().reduce(CGFloat.zero) {
            $0 + $1.width
        }
        let spacingWidth = intercellSpacing.width * CGFloat(max(0, tableColumns.count - 1))
        let leadingWidth = max(
            leadingColumn.minWidth,
            bounds.width - fixedColumnWidth - spacingWidth
        )
        guard abs(leadingColumn.width - leadingWidth) > 0.5 else { return }
        leadingColumn.width = leadingWidth
    }

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
