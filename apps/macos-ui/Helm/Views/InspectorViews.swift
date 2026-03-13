import SwiftUI
import AppKit

// swiftlint:disable file_length

struct ControlCenterInspectorView: View {
    @ObservedObject private var core = HelmCore.shared
    @EnvironmentObject private var context: ControlCenterContext

    private var selectedTask: TaskItem? {
        guard let taskId = context.selectedTaskId else { return nil }
        return core.activeTasks.first { $0.id == taskId }
    }

    private var selectedPackage: PackageItem? {
        guard let packageId = context.selectedPackageId else { return nil }
        return core.knownPackage(withId: packageId)
    }

    private var selectedUpgradePlanTask: TaskItem? {
        guard let stepId = context.selectedUpgradePlanStepId,
              let step = core.upgradePlanSteps.first(where: { $0.id == stepId }) else { return nil }
        let projectedStatus = core.projectedUpgradePlanStatus(for: step)
        let projectedTaskId = core.projectedUpgradePlanTaskId(for: step)
        let taskId = projectedTaskId.map(String.init) ?? step.id
        var labelArgs = step.reasonLabelArgs
        labelArgs["plan_step_id"] = step.id
        return TaskItem(
            id: taskId,
            description: core.localizedUpgradePlanReason(for: step),
            status: projectedStatus,
            managerId: step.managerId,
            taskType: step.action,
            labelKey: step.reasonLabelKey,
            labelArgs: labelArgs
        )
    }

    private var selectedManager: ManagerInfo? {
        guard let managerId = context.selectedManagerId else { return nil }
        return ManagerInfo.all.first { $0.id == managerId }
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                Text(L10n.App.Inspector.title.localized)
                    .font(.headline)

                if let task = selectedTask ?? selectedUpgradePlanTask {
                    InspectorTaskDetailView(task: task)
                } else if let package = selectedPackage {
                    let runtimeStateToken = [
                        package.runtimeState.isActive ? "1" : "0",
                        package.runtimeState.isDefault ? "1" : "0",
                        package.runtimeState.hasOverride ? "1" : "0",
                    ].joined()
                    let packageInspectorToken = "\(package.id)|\(package.pinned ? 1 : 0)|\(package.version)|\(package.latestVersion ?? "")|\(runtimeStateToken)"
                    InspectorPackageDetailView(package: package)
                        .id(packageInspectorToken)
                } else if let manager = selectedManager {
                    InspectorManagerDetailView(
                        manager: manager,
                        status: core.managerStatuses[manager.id],
                        detectionDiagnostics: core.managerDetectionDiagnostics(for: manager.id),
                        health: core.health(forManagerId: manager.id),
                        packageCount: core.installedPackages.filter { $0.managerId == manager.id }.count,
                        outdatedCount: core.outdatedCount(forManagerId: manager.id)
                    )
                } else {
                    Text(L10n.App.Inspector.empty.localized)
                        .font(.callout)
                        .foregroundColor(.secondary)
                }

                Spacer()
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(14)
        }
    }
}

// MARK: - Task Inspector

private struct InspectorTaskDetailView: View {
    @ObservedObject private var core = HelmCore.shared
    @State private var showDiagnosticsSheet = false
    @State private var isLoadingTaskOutput = false
    @State private var taskOutputLoadFailed = false
    @State private var taskOutputRecord: CoreTaskOutputRecord?
    @State private var isLoadingTaskLogs = false
    @State private var taskLogsLoadFailed = false
    @State private var taskLogRecords: [CoreTaskLogRecord] = []
    @State private var taskLogFetchLimit = Self.taskLogPageSize
    private static let taskLogPageSize = 50
    let task: TaskItem

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(task.description)
                .font(.title3.weight(.semibold))

            // Status badge
            HStack(spacing: 6) {
                Image(systemName: task.statusIcon)
                    .foregroundColor(task.statusColor)
                Text(task.localizedStatus)
                    .font(.callout.weight(.medium))
                    .foregroundColor(task.statusColor)
            }
            .accessibilityElement(children: .combine)
            .accessibilityLabel(L10n.App.Inspector.taskStatus.localized)
            .accessibilityValue(task.localizedStatus)

            InspectorField(label: L10n.App.Inspector.taskId.localized) {
                Text(task.id)
                    .font(.caption.monospacedDigit())
                    .accessibilityLabel(L10n.App.Inspector.taskId.localized)
                    .accessibilityValue(task.id)
            }

            if let taskType = task.taskType {
                InspectorField(label: L10n.App.Inspector.taskType.localized) {
                    Text(localizedTaskType(taskType))
                        .font(.callout)
                }
            }

            if let managerId = task.managerId {
                InspectorField(label: L10n.App.Inspector.taskManager.localized) {
                    Text(localizedManagerDisplayName(managerId))
                        .font(.callout)
                }
            }

            InspectorField(label: L10n.App.Inspector.taskCommand.localized) {
                Text(taskCommandText())
                    .font(.caption.monospacedDigit())
                    .foregroundColor(diagnosticCommandHint() == nil ? .secondary : .primary)
            }

            if let labelKey = task.labelKey {
                InspectorField(label: L10n.App.Inspector.taskLabelKey.localized) {
                    Text(labelKey)
                        .font(.caption.monospacedDigit())
                        .foregroundColor(.secondary)
                        .accessibilityLabel(L10n.App.Inspector.taskLabelKey.localized)
                        .accessibilityValue(labelKey)
                }
            }

            if let labelArgs = task.labelArgs, !labelArgs.isEmpty {
                InspectorField(label: L10n.App.Inspector.taskLabelArgs.localized) {
                    VStack(alignment: .leading, spacing: 2) {
                        ForEach(labelArgs.sorted(by: { $0.key < $1.key }), id: \.key) { key, value in
                            HStack(spacing: 4) {
                                Text(key)
                                    .font(.caption.monospacedDigit())
                                    .foregroundColor(.secondary)
                                Text(":")
                                    .font(.caption)
                                    .foregroundColor(.secondary)
                                Text(value)
                                    .font(.caption.monospacedDigit())
                            }
                        }
                    }
                }
            }

            if task.status.lowercased() == "failed" {
                InspectorField(label: L10n.App.Inspector.taskFailureFeedback.localized) {
                    VStack(alignment: .leading, spacing: 8) {
                        Text(failureHintText())
                            .font(.caption)
                            .foregroundColor(.secondary)

                        if hasNumericTaskId {
                            Button(L10n.App.Inspector.viewDiagnostics.localized) {
                                showDiagnosticsSheet = true
                                loadTaskOutput(force: true)
                                loadTaskLogs(force: true, resetPagination: true)
                            }
                            .buttonStyle(HelmSecondaryButtonStyle())
                            .font(.caption)
                            .helmPointer()
                        }
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .popover(isPresented: $showDiagnosticsSheet, arrowEdge: .leading) {
            TaskDiagnosticsSheetView(
                taskDescription: task.description,
                diagnosticsText: HelmSupport.generateTaskDiagnostics(
                    task: task,
                    suggestedCommand: diagnosticCommandHint(),
                    output: taskOutputRecord
                ),
                output: taskOutputRecord,
                isLoading: isLoadingTaskOutput,
                loadFailed: taskOutputLoadFailed,
                logs: taskLogRecords,
                isLoadingLogs: isLoadingTaskLogs,
                logsLoadFailed: taskLogsLoadFailed,
                canLoadMoreLogs: taskLogRecords.count >= taskLogFetchLimit,
                onLoadMoreLogs: loadMoreTaskLogs
            )
            .frame(minWidth: 700, minHeight: 420)
        }
    }

    private func localizedTaskType(_ rawType: String) -> String {
        HelmCore.shared.localizedTaskType(rawType)
    }

    private func failureHintText() -> String {
        if task.labelKey == "service.task.label.install.homebrew_formula",
           let package = task.labelArgs?["package"] {
            return L10n.App.Inspector.taskFailureHintHomebrewInstall.localized(with: [
                "package": package
            ])
        }

        if let managerId = task.managerId {
            return L10n.App.Inspector.taskFailureHintGeneric.localized(with: [
                "manager": localizedManagerDisplayName(managerId)
            ])
        }

        return L10n.Service.Error.processFailure.localized
    }

    private func diagnosticCommandHint() -> String? {
        core.diagnosticCommandHint(for: task)
    }

    private func taskCommandText() -> String {
        diagnosticCommandHint() ?? L10n.App.Inspector.taskCommandUnavailable.localized
    }

    private var hasNumericTaskId: Bool {
        Int64(task.id) != nil
    }

    private func loadTaskOutput(force: Bool) {
        guard hasNumericTaskId else { return }
        if isLoadingTaskOutput {
            return
        }
        if taskOutputRecord != nil && !force {
            return
        }

        isLoadingTaskOutput = true
        taskOutputLoadFailed = false
        core.fetchTaskOutput(taskId: task.id) { output in
            DispatchQueue.main.async {
                self.isLoadingTaskOutput = false
                if let output {
                    self.taskOutputRecord = output
                    self.taskOutputLoadFailed = false
                } else {
                    self.taskOutputRecord = nil
                    self.taskOutputLoadFailed = true
                }
            }
        }
    }

    private func loadTaskLogs(force: Bool, resetPagination: Bool = false) {
        guard hasNumericTaskId else { return }
        if resetPagination {
            taskLogFetchLimit = Self.taskLogPageSize
        }
        if isLoadingTaskLogs {
            return
        }
        if !force && !taskLogRecords.isEmpty {
            return
        }

        isLoadingTaskLogs = true
        taskLogsLoadFailed = false
        core.fetchTaskLogs(taskId: task.id, limit: taskLogFetchLimit) { logs in
            DispatchQueue.main.async {
                self.isLoadingTaskLogs = false
                if let logs {
                    self.taskLogRecords = logs
                    self.taskLogsLoadFailed = false
                } else {
                    if resetPagination {
                        self.taskLogRecords = []
                    }
                    self.taskLogsLoadFailed = true
                }
            }
        }
    }

    private func loadMoreTaskLogs() {
        guard !isLoadingTaskLogs else { return }
        taskLogFetchLimit += Self.taskLogPageSize
        loadTaskLogs(force: true)
    }
}

private struct TaskDiagnosticsSheetView: View {
    let taskDescription: String
    let diagnosticsText: String
    let output: CoreTaskOutputRecord?
    let isLoading: Bool
    let loadFailed: Bool
    let logs: [CoreTaskLogRecord]
    let isLoadingLogs: Bool
    let logsLoadFailed: Bool
    let canLoadMoreLogs: Bool
    let onLoadMoreLogs: () -> Void
    @State private var levelFilter: TaskLogLevelFilter = .all
    @State private var statusFilter: TaskLogStatusFilter = .all

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(L10n.App.Inspector.taskDiagnostics.localized)
                .font(.headline)

            Text(taskDescription)
                .font(.subheadline)
                .foregroundColor(.secondary)
                .lineLimit(2)

            TabView {
                TaskOutputTextView(
                    text: diagnosticsText,
                    unavailableText: L10n.App.Inspector.taskDiagnosticsUnavailable.localized
                )
                .tabItem { Text(L10n.App.Inspector.taskOutputDiagnostics.localized) }

                TaskOutputTextView(
                    text: output?.stderr,
                    unavailableText: streamUnavailableText()
                )
                .tabItem { Text(L10n.App.Inspector.taskOutputStderr.localized) }

                TaskOutputTextView(
                    text: output?.stdout,
                    unavailableText: streamUnavailableText()
                )
                .tabItem { Text(L10n.App.Inspector.taskOutputStdout.localized) }

                TaskLogListView(
                    logs: logs,
                    levelFilter: $levelFilter,
                    statusFilter: $statusFilter,
                    isLoading: isLoadingLogs,
                    loadFailed: logsLoadFailed,
                    canLoadMore: canLoadMoreLogs,
                    onLoadMore: onLoadMoreLogs
                )
                .tabItem { Text(L10n.App.Inspector.taskOutputLogs.localized) }
            }
        }
        .padding(16)
    }

    private func streamUnavailableText() -> String {
        if isLoading && output == nil {
            return L10n.App.Inspector.taskOutputLoading.localized
        }
        if loadFailed {
            return L10n.App.Inspector.taskOutputLoadFailed.localized
        }
        return L10n.App.Inspector.taskOutputUnavailable.localized
    }
}

private enum TaskLogLevelFilter: String, CaseIterable, Identifiable {
    case all
    case info
    case warn
    case error

    var id: String { rawValue }

    var localizedTitle: String {
        switch self {
        case .all:
            return L10n.App.Inspector.taskLogsLevelAll.localized
        case .info:
            return L10n.App.Inspector.taskLogsLevelInfo.localized
        case .warn:
            return L10n.Common.warning.localized
        case .error:
            return L10n.Common.error.localized
        }
    }

    func matches(level: String) -> Bool {
        switch self {
        case .all:
            return true
        default:
            return level.lowercased() == rawValue
        }
    }
}

private enum TaskLogStatusFilter: String, CaseIterable, Identifiable {
    case all
    case queued
    case running
    case completed
    case cancelled
    case failed

    var id: String { rawValue }

    var localizedTitle: String {
        switch self {
        case .all:
            return L10n.App.Inspector.taskLogsStatusAll.localized
        case .queued:
            return L10n.Service.Task.Status.pending.localized
        case .running:
            return L10n.Service.Task.Status.running.localized
        case .completed:
            return L10n.Service.Task.Status.completed.localized
        case .cancelled:
            return L10n.Service.Task.Status.cancelled.localized
        case .failed:
            return L10n.Service.Task.Status.failed.localized
        }
    }

    func matches(status: String?) -> Bool {
        switch self {
        case .all:
            return true
        default:
            return status?.lowercased() == rawValue
        }
    }
}

private struct TaskOutputTextView: View {
    let text: String?
    let unavailableText: String

    private var normalizedText: String? {
        guard let text = text?.trimmingCharacters(in: .whitespacesAndNewlines),
              !text.isEmpty else {
            return nil
        }
        return text
    }

    private var copyText: String? {
        normalizedText
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Spacer()
                Button {
                    if let copyText {
                        copyTextToClipboard(copyText)
                    }
                } label: {
                    Label(L10n.App.Inspector.copyAll.localized, systemImage: "doc.on.doc")
                }
                .buttonStyle(HelmSecondaryButtonStyle(cornerRadius: 8, horizontalPadding: 8, verticalPadding: 4))
                .font(.caption)
                .disabled(copyText == nil)
                .helmPointer(enabled: copyText != nil)
            }

            if let normalizedText {
                SelectableMonospacedTextArea(text: normalizedText)
                    .frame(minHeight: 180)
                    .background(
                        RoundedRectangle(cornerRadius: 8)
                            .fill(Color(NSColor.textBackgroundColor))
                    )
                    .frame(maxWidth: .infinity, alignment: .leading)
            } else {
                Text(unavailableText)
                    .foregroundColor(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }

            Spacer(minLength: 0)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }
}

private struct TaskLogListView: View {
    let logs: [CoreTaskLogRecord]
    @Binding var levelFilter: TaskLogLevelFilter
    @Binding var statusFilter: TaskLogStatusFilter
    let isLoading: Bool
    let loadFailed: Bool
    let canLoadMore: Bool
    let onLoadMore: () -> Void

    private var filteredLogs: [CoreTaskLogRecord] {
        logs.filter { entry in
            levelFilter.matches(level: entry.level)
            && statusFilter.matches(status: entry.status)
        }
    }

    private static let timestampFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.dateStyle = .none
        formatter.timeStyle = .medium
        return formatter
    }()

    private var filteredLogsText: String? {
        guard !filteredLogs.isEmpty else { return nil }
        return filteredLogs
            .map { entry in
                let timestamp = Self.timestampFormatter.string(from: entry.createdAtDate)
                let level = entry.level.uppercased()
                let rawStatus = entry.status?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
                let status = (rawStatus.isEmpty ? "-" : rawStatus).uppercased()
                let manager = localizedManagerDisplayName(entry.manager)
                return "[\(timestamp)] [\(level)] [\(status)] [\(manager)]\n\(entry.message)"
            }
            .joined(separator: "\n\n")
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 10) {
                Picker(L10n.App.Inspector.taskLogsLevelFilter.localized, selection: $levelFilter) {
                    ForEach(TaskLogLevelFilter.allCases) { filter in
                        Text(filter.localizedTitle).tag(filter)
                    }
                }
                .frame(maxWidth: 220)

                Picker(L10n.App.Inspector.taskLogsStatusFilter.localized, selection: $statusFilter) {
                    ForEach(TaskLogStatusFilter.allCases) { filter in
                        Text(filter.localizedTitle).tag(filter)
                    }
                }
                .frame(maxWidth: 220)

                Spacer()

                Button {
                    if let filteredLogsText {
                        copyTextToClipboard(filteredLogsText)
                    }
                } label: {
                    Label(L10n.App.Inspector.copyAll.localized, systemImage: "doc.on.doc")
                }
                .buttonStyle(HelmSecondaryButtonStyle(cornerRadius: 8, horizontalPadding: 8, verticalPadding: 4))
                .font(.caption)
                .disabled(filteredLogsText == nil)
                .helmPointer(enabled: filteredLogsText != nil)
            }

            Group {
                if isLoading && logs.isEmpty {
                    Text(L10n.App.Inspector.taskOutputLoading.localized)
                        .foregroundColor(.secondary)
                } else if loadFailed && logs.isEmpty {
                    Text(L10n.App.Inspector.taskOutputLoadFailed.localized)
                        .foregroundColor(.secondary)
                } else if logs.isEmpty {
                    Text(L10n.App.Inspector.taskLogsEmpty.localized)
                        .foregroundColor(.secondary)
                } else if filteredLogs.isEmpty {
                    Text(L10n.App.Inspector.taskLogsEmptyFiltered.localized)
                        .foregroundColor(.secondary)
                } else {
                    SelectableMonospacedTextArea(text: filteredLogsText ?? "")
                        .frame(minHeight: 220)
                        .background(
                            RoundedRectangle(cornerRadius: 8)
                                .fill(Color(NSColor.textBackgroundColor))
                        )
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            if canLoadMore && !isLoading {
                Button(L10n.App.Inspector.taskLogsLoadMore.localized) {
                    onLoadMore()
                }
                .buttonStyle(HelmSecondaryButtonStyle())
                .font(.caption)
                .helmPointer()
            }

            Spacer(minLength: 0)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }
}

private struct SelectableMonospacedTextArea: NSViewRepresentable {
    let text: String

    func makeNSView(context: Context) -> NSScrollView {
        let scrollView = NSScrollView()
        scrollView.hasVerticalScroller = true
        scrollView.hasHorizontalScroller = true
        scrollView.autohidesScrollers = true
        scrollView.borderType = .noBorder
        scrollView.drawsBackground = false

        let textView = CopyableTextView()
        textView.isEditable = false
        textView.isSelectable = true
        textView.drawsBackground = false
        textView.isRichText = false
        textView.usesFindBar = true
        textView.font = NSFont.monospacedSystemFont(ofSize: NSFont.smallSystemFontSize, weight: .regular)
        textView.textColor = NSColor.labelColor
        textView.textContainerInset = NSSize(width: 8, height: 8)
        textView.textContainer?.lineFragmentPadding = 0
        textView.isHorizontallyResizable = true
        textView.isVerticallyResizable = true
        textView.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        textView.textContainer?.containerSize = NSSize(
            width: CGFloat.greatestFiniteMagnitude,
            height: CGFloat.greatestFiniteMagnitude
        )
        textView.textContainer?.widthTracksTextView = false
        textView.textContainer?.heightTracksTextView = false
        textView.string = text

        scrollView.documentView = textView
        return scrollView
    }

    func updateNSView(_ nsView: NSScrollView, context: Context) {
        guard let textView = nsView.documentView as? NSTextView else { return }
        if textView.string != text {
            textView.string = text
        }
    }
}

private final class CopyableTextView: NSTextView {
    override var acceptsFirstResponder: Bool { true }

    override func keyDown(with event: NSEvent) {
        if event.modifierFlags.intersection(.deviceIndependentFlagsMask) == [.command],
           event.charactersIgnoringModifiers?.lowercased() == "c" {
            copy(self)
            return
        }
        super.keyDown(with: event)
    }
}

private func copyTextToClipboard(_ text: String) {
    let pasteboard = NSPasteboard.general
    pasteboard.clearContents()
    pasteboard.setString(text, forType: .string)
}

// MARK: - Package Inspector

private struct InspectorPackageDetailView: View {
    @ObservedObject private var core = HelmCore.shared
    @EnvironmentObject private var context: ControlCenterContext
    @State private var loadingPackageUninstallPreview = false
    @State private var inspectorAlert: InspectorPackageAlert?
    let package: PackageItem

    private static let unknownVersionTokens: Set<String> = {
        var tokens: Set<String> = ["unknown"]
        let localizedUnknown = L10n.Common.unknown.localized
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()
        if !localizedUnknown.isEmpty {
            tokens.insert(localizedUnknown)
        }
        return tokens
    }()

    private struct ManagerSwitchAlertContext: Identifiable {
        let packageName: String
        let selectedManagerId: String
        let title: String
        let message: String

        var id: String {
            "\(packageName.lowercased())-\(selectedManagerId)"
        }
    }

    private enum InspectorPackageAlert: Identifiable {
        case uninstall(package: PackageItem, preview: PackageUninstallPreview)
        case uninstallFallback(package: PackageItem)
        case switchManager(ManagerSwitchAlertContext)

        var id: String {
            switch self {
            case let .uninstall(package, preview):
                return "uninstall-\(package.id)-\(preview.blastRadiusScore)"
            case let .uninstallFallback(package):
                return "uninstall-fallback-\(package.id)"
            case let .switchManager(context):
                return "switch-manager-\(context.id)"
            }
        }
    }

    private struct ManagerPackageGroup: Identifiable {
        let managerId: String
        let packages: [PackageItem]

        var id: String { managerId }

        var representativePackage: PackageItem? {
            packages.first
        }
    }

    private var livePackage: PackageItem {
        core.knownPackage(withId: package.id) ?? package
    }

    private var managerGroups: [ManagerPackageGroup] {
        let grouped = Dictionary(grouping: packageFamilyMembers(for: livePackage), by: \.managerId)
        return grouped.compactMap { managerId, candidates in
            let sortedCandidates = candidates.sorted(by: memberCandidateOrdering)
            guard !sortedCandidates.isEmpty else { return nil }
            return ManagerPackageGroup(managerId: managerId, packages: sortedCandidates)
        }
        .sorted { lhs, rhs in
            guard let lhsRepresentative = lhs.representativePackage,
                  let rhsRepresentative = rhs.representativePackage else {
                return lhs.managerId < rhs.managerId
            }
            return managerCandidateOrdering(lhsRepresentative, rhsRepresentative)
        }
    }

    private var managerCandidates: [PackageItem] {
        managerGroups.compactMap(\.representativePackage)
    }

    private var installedManagerCandidates: [PackageItem] {
        managerCandidates.filter { $0.status != .available }
    }

    private var installedProvenanceManagerPackage: PackageItem? {
        installedManagerCandidates.first
    }

    private var recommendedManagerPackage: PackageItem {
        if let preferredInstalled = installedProvenanceManagerPackage {
            return preferredInstalled
        }
        if let preferredAvailable = managerCandidates.first(where: { $0.status == .available }) {
            return preferredAvailable
        }
        return managerCandidates.first ?? livePackage
    }

    private var selectedManagerId: String {
        if let preferredManagerId = core.preferredManagerId(for: livePackage),
           managerCandidates.contains(where: { $0.managerId == preferredManagerId }) {
            return preferredManagerId
        }
        return recommendedManagerPackage.managerId
    }

    private var activeManagerGroup: ManagerPackageGroup? {
        managerGroups.first(where: { $0.managerId == selectedManagerId })
    }

    private var activePackage: PackageItem {
        if let activeManagerGroup,
           let selectedPackageId = context.selectedPackageId,
           let selectedPackage = activeManagerGroup.packages.first(where: { $0.id == selectedPackageId }) {
            return selectedPackage
        }
        return activeManagerGroup?.representativePackage ?? recommendedManagerPackage
    }

    private var supportsKegPolicyOverride: Bool {
        activePackage.managerId == "homebrew_formula" && activePackage.status != .available
    }

    private var kegPolicySelection: KegPolicySelection {
        core.kegPolicySelection(for: activePackage)
    }

    private var currentVersionText: String {
        normalizedVersionText(activePackage.version) ?? L10n.Common.unknown.localized
    }

    private struct RuntimeStateBadge: Identifiable {
        let id: String
        let title: String
        let color: Color
    }

    private var runtimeStateBadges: [RuntimeStateBadge] {
        var badges: [RuntimeStateBadge] = []
        if activePackage.runtimeState.isActive {
            badges.append(
                RuntimeStateBadge(
                    id: "active",
                    title: L10n.App.Inspector.packageRuntimeStateActive.localized,
                    color: .green
                )
            )
        }
        if activePackage.runtimeState.isDefault {
            badges.append(
                RuntimeStateBadge(
                    id: "default",
                    title: L10n.App.Inspector.packageRuntimeStateDefault.localized,
                    color: .blue
                )
            )
        }
        if activePackage.runtimeState.hasOverride {
            badges.append(
                RuntimeStateBadge(
                    id: "override",
                    title: L10n.App.Inspector.packageRuntimeStateOverride.localized,
                    color: .orange
                )
            )
        }
        return badges
    }

    private var latestVersionText: String? {
        guard activePackage.status == .upgradable else { return nil }
        return normalizedVersionText(activePackage.latestVersion)
    }

    private var resolvedPackageSummary: String? {
        core.packageDescriptionSummary(for: activePackage)
    }

    private var shouldShowRustupToolchainDetail: Bool {
        activePackage.managerId == "rustup" && activePackage.status != .available
    }

    private var shouldShowManagerSpecificPackageSection: Bool {
        shouldShowRustupToolchainDetail
    }

    private var rustupToolchainDetail: CoreRustupToolchainDetail? {
        core.rustupToolchainDetail(for: activePackage)
    }

    private var rustupToolchainActionInFlight: Bool {
        core.isRustupToolchainActionInFlight(for: activePackage)
    }

    private var renderedPackageDescription: PackageDescriptionRenderer.RenderedDescription? {
        core.renderedPackageDescription(for: activePackage, summaryOverride: resolvedPackageSummary)
    }

    private var descriptionLoadingView: some View {
        HStack(spacing: 6) {
            ProgressView()
                .controlSize(.small)
                .scaleEffect(0.8)
            Text(L10n.App.Inspector.descriptionLoading.localized)
                .font(.caption)
                .foregroundColor(.secondary)
        }
    }

    private var rustupToolchainDetailLoadingView: some View {
        HStack(spacing: 6) {
            ProgressView()
                .controlSize(.small)
                .scaleEffect(0.8)
            Text(L10n.App.Inspector.rustupDetailLoading.localized)
                .font(.caption)
                .foregroundColor(.secondary)
        }
    }

    private var rustupToolchainMutationLoadingView: some View {
        HStack(spacing: 6) {
            ProgressView()
                .controlSize(.small)
                .scaleEffect(0.8)
            Text(L10n.App.Inspector.rustupConfiguring.localized)
                .font(.caption)
                .foregroundColor(.secondary)
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(activePackage.displayName)
                    .font(.title3.weight(.semibold))
                Text(currentVersionText)
                    .font(.caption.monospacedDigit())
                    .foregroundColor(.secondary)
                if !runtimeStateBadges.isEmpty {
                    HStack(spacing: 6) {
                        ForEach(runtimeStateBadges) { badge in
                            Text(badge.title)
                                .font(.caption2.weight(.medium))
                                .padding(.horizontal, 6)
                                .padding(.vertical, 2)
                                .background(
                                    Capsule(style: .continuous)
                                        .fill(badge.color.opacity(0.14))
                                )
                                .foregroundColor(badge.color)
                        }
                    }
                    .accessibilityElement(children: .combine)
                    .accessibilityLabel(L10n.App.Inspector.packageRuntimeState.localized)
                }
            }

            if let latestVersionText {
                InspectorField(label: L10n.App.Inspector.latest.localized) {
                    Text(latestVersionText)
                        .font(.caption.monospacedDigit())
                }
            }

            Group {
                if let renderedPackageDescription {
                    switch renderedPackageDescription {
                    case .plain(let text):
                        Text(text)
                            .font(.caption)
                    case .rich(let attributed):
                        InspectorAttributedText(attributedText: attributed)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                } else if core.packageDescriptionLoadingIds.contains(activePackage.id) {
                    descriptionLoadingView
                } else if core.packageDescriptionUnavailableIds.contains(activePackage.id) {
                    Text(L10n.App.Inspector.descriptionUnavailable.localized)
                        .font(.caption)
                        .foregroundColor(.secondary)
                } else {
                    descriptionLoadingView
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            packageActionRow

            if supportsKegPolicyOverride {
                kegPolicyMenuField
            }

            managerSelectionField

            if shouldShowManagerSpecificPackageSection {
                Divider()
                    .padding(.vertical, 4)
            }

            if shouldShowRustupToolchainDetail {
                rustupToolchainDetailSection
            }

            packageIdField
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .onAppear {
            ensurePersistedManagerSelection()
            ensureSelectedPackageMember()
            core.ensurePackageDescription(for: activePackage)
            core.ensureRustupToolchainDetail(for: activePackage)
        }
        .onChange(of: package.id) { _ in
            ensurePersistedManagerSelection()
            ensureSelectedPackageMember()
            core.ensurePackageDescription(for: activePackage)
            core.ensureRustupToolchainDetail(for: activePackage)
        }
        .onChange(of: core.managerStatuses.mapValues(\.enabled)) { _ in
            ensurePersistedManagerSelection()
            ensureSelectedPackageMember()
        }
        .onChange(of: package.summary) { _ in
            core.ensurePackageDescription(for: activePackage)
        }
        .onChange(of: selectedManagerId) { _ in
            ensureSelectedPackageMember()
            core.ensurePackageDescription(for: activePackage)
            core.ensureRustupToolchainDetail(for: activePackage)
        }
        .alert(item: $inspectorAlert) { action in
            switch action {
            case let .uninstall(targetPackage, preview):
                let message = packageUninstallAlertMessage(preview, package: targetPackage)
                if preview.managerAutomationLevel == "read_only" {
                    return Alert(
                        title: Text(
                            L10n.App.Packages.Alert.uninstallTitle.localized(
                                with: ["package": targetPackage.displayName]
                            )
                        ),
                        message: Text(message),
                        dismissButton: .default(Text(L10n.Common.ok.localized))
                    )
                }
                return Alert(
                        title: Text(
                            L10n.App.Packages.Alert.uninstallTitle.localized(
                                with: ["package": targetPackage.displayName]
                            )
                        ),
                        message: Text(message),
                        primaryButton: .destructive(Text(L10n.Common.uninstall.localized)) {
                            core.uninstallPackage(targetPackage)
                        },
                        secondaryButton: .cancel()
                    )
            case let .uninstallFallback(targetPackage):
                return Alert(
                    title: Text(
                        L10n.App.Packages.Alert.uninstallTitle.localized(
                            with: ["package": targetPackage.displayName]
                        )
                    ),
                    message: Text(
                        L10n.App.Packages.Alert.uninstallMessage.localized(
                            with: [
                                "package": targetPackage.displayName,
                                "manager": localizedManagerDisplayName(targetPackage.managerId),
                            ]
                        )
                    ),
                    primaryButton: .destructive(Text(L10n.Common.uninstall.localized)) {
                        core.uninstallPackage(targetPackage)
                    },
                    secondaryButton: .cancel()
                )
            case let .switchManager(switchContext):
                return Alert(
                    title: Text(switchContext.title),
                    message: Text(switchContext.message),
                    primaryButton: .default(Text(L10n.Common.continue.localized)) {
                        persistManagerSelection(switchContext.selectedManagerId)
                    },
                    secondaryButton: .cancel()
                )
            }
        }
    }

    @ViewBuilder
    private var rustupToolchainDetailSection: some View {
        if let detail = rustupToolchainDetail {
            if rustupToolchainActionInFlight {
                rustupToolchainMutationLoadingView
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            rustupProfileField(detail)
            rustupOverridesField(detail)
            rustupToolchainEntryField(
                label: L10n.App.Inspector.rustupComponents.localized,
                entries: detail.components,
                addLabel: L10n.App.Inspector.rustupAddComponent.localized,
                removeLabel: L10n.App.Inspector.rustupRemoveComponent.localized,
                onAdd: { component in
                    core.addRustupComponent(component, to: activePackage)
                },
                onRemove: { component in
                    core.removeRustupComponent(component, from: activePackage)
                }
            )
            rustupToolchainEntryField(
                label: L10n.App.Inspector.rustupTargets.localized,
                entries: detail.targets,
                addLabel: L10n.App.Inspector.rustupAddTarget.localized,
                removeLabel: L10n.App.Inspector.rustupRemoveTarget.localized,
                onAdd: { target in
                    core.addRustupTarget(target, to: activePackage)
                },
                onRemove: { target in
                    core.removeRustupTarget(target, from: activePackage)
                }
            )
        } else if core.isRustupToolchainDetailLoading(for: activePackage) {
            rustupToolchainDetailLoadingView
        } else if core.isRustupToolchainDetailUnavailable(for: activePackage) {
            Text(L10n.App.Inspector.rustupDetailUnavailable.localized)
                .font(.caption)
                .foregroundColor(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
        } else {
            rustupToolchainDetailLoadingView
        }
    }

    private func rustupToolchainEntryField(
        label: String,
        entries: [CoreRustupToolchainDetailEntry],
        addLabel: String,
        removeLabel: String,
        onAdd: @escaping (String) -> Void,
        onRemove: @escaping (String) -> Void
    ) -> some View {
        let installedEntries = entries.filter(\.installed)
        let availableEntries = entries.filter { !$0.installed }
        return InspectorField(label: label) {
            VStack(alignment: .leading, spacing: 6) {
                HStack(alignment: .center, spacing: 8) {
                    Text(
                        L10n.App.Inspector.rustupInstalledOfAvailable.localized(
                            with: [
                                "installed": "\(installedEntries.count)",
                                "available": "\(entries.count)",
                            ]
                        )
                    )
                    .font(.caption)
                    .foregroundColor(.secondary)

                    Spacer(minLength: 0)

                    if !availableEntries.isEmpty {
                        Menu {
                            ForEach(availableEntries) { entry in
                                Button(entry.name) {
                                    onAdd(entry.name)
                                }
                            }
                        } label: {
                            rustupToolchainMenuLabel(addLabel)
                        }
                        .buttonStyle(HelmSecondaryButtonStyle(cornerRadius: 8, horizontalPadding: 8, verticalPadding: 4))
                        .disabled(rustupToolchainActionInFlight)
                    }

                    if !installedEntries.isEmpty {
                        Menu {
                            ForEach(installedEntries) { entry in
                                Button(entry.name) {
                                    onRemove(entry.name)
                                }
                            }
                        } label: {
                            rustupToolchainMenuLabel(removeLabel)
                        }
                        .buttonStyle(HelmSecondaryButtonStyle(cornerRadius: 8, horizontalPadding: 8, verticalPadding: 4))
                        .disabled(rustupToolchainActionInFlight)
                    }
                }

                if installedEntries.isEmpty {
                    Text(L10n.App.Inspector.rustupNoneInstalled.localized)
                        .font(.caption)
                        .foregroundColor(.secondary)
                } else {
                    LazyVGrid(
                        columns: [GridItem(.adaptive(minimum: 118), spacing: 6)],
                        alignment: .leading,
                        spacing: 6
                    ) {
                        ForEach(installedEntries) { entry in
                            rustupToolchainEntryBadge(entry.name)
                        }
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func rustupProfileField(_ detail: CoreRustupToolchainDetail) -> some View {
        let currentProfile = detail.currentProfile?.trimmingCharacters(in: .whitespacesAndNewlines)
        let normalizedCurrentProfile = currentProfile?.lowercased()
        let profiles = ["minimal", "default", "complete"]
        return InspectorField(label: L10n.App.Inspector.rustupProfile.localized) {
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 8) {
                    Text(currentProfile?.isEmpty == false ? currentProfile! : "-")
                        .font(.callout)
                    Spacer(minLength: 0)

                    Menu {
                        ForEach(profiles, id: \.self) { profile in
                            Button {
                                core.setRustupProfile(profile, for: activePackage)
                            } label: {
                                HStack(spacing: 8) {
                                    Text(profile)
                                    if normalizedCurrentProfile == profile {
                                        Image(systemName: "checkmark")
                                    }
                                }
                            }
                        }
                    } label: {
                        rustupToolchainMenuLabel(L10n.App.Inspector.rustupSetProfile.localized)
                    }
                    .buttonStyle(HelmSecondaryButtonStyle(cornerRadius: 8, horizontalPadding: 8, verticalPadding: 4))
                    .disabled(rustupToolchainActionInFlight)

                    Button(L10n.App.Inspector.rustupSetDefault.localized) {
                        core.setRustupDefaultToolchain(activePackage)
                    }
                    .buttonStyle(HelmSecondaryButtonStyle(cornerRadius: 8, horizontalPadding: 8, verticalPadding: 4))
                    .disabled(rustupToolchainActionInFlight || activePackage.runtimeState.isDefault)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func rustupOverridesField(_ detail: CoreRustupToolchainDetail) -> some View {
        InspectorField(label: L10n.App.Inspector.rustupOverrides.localized) {
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 8) {
                    Text(
                        detail.overridePaths.isEmpty
                            ? L10n.App.Inspector.rustupNoOverrides.localized
                            : L10n.App.Inspector.rustupOverridesConfigured.localized(
                                with: ["count": "\(detail.overridePaths.count)"]
                            )
                    )
                    .font(.caption)
                    .foregroundColor(.secondary)

                    Spacer(minLength: 0)

                    Button(L10n.App.Inspector.rustupSetOverride.localized) {
                        pickRustupOverrideDirectory(for: activePackage)
                    }
                    .buttonStyle(HelmSecondaryButtonStyle(cornerRadius: 8, horizontalPadding: 8, verticalPadding: 4))
                    .disabled(rustupToolchainActionInFlight)
                }

                if detail.overridePaths.isEmpty {
                    Text(L10n.App.Inspector.rustupNoOverrides.localized)
                        .font(.caption)
                        .foregroundColor(.secondary)
                } else {
                    ForEach(detail.overridePaths, id: \.self) { path in
                        HStack(alignment: .top, spacing: 8) {
                            Text(path)
                                .font(.system(.caption, design: .monospaced))
                                .foregroundColor(.secondary)
                                .frame(maxWidth: .infinity, alignment: .leading)

                            Button {
                                core.unsetRustupOverride(activePackage, path: path)
                            } label: {
                                Image(systemName: "xmark.circle")
                            }
                            .buttonStyle(HelmIconButtonStyle())
                            .help(L10n.App.Inspector.rustupClearOverride.localized)
                            .disabled(rustupToolchainActionInFlight)
                        }
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func rustupToolchainEntryBadge(_ text: String) -> some View {
        Text(text)
            .font(.caption2)
            .padding(.horizontal, 6)
            .padding(.vertical, 3)
            .background(
                Capsule(style: .continuous)
                    .fill(HelmTheme.surfaceElevated)
                    .overlay(
                        Capsule(style: .continuous)
                            .strokeBorder(HelmTheme.borderSubtle.opacity(0.9), lineWidth: 0.8)
                    )
            )
            .foregroundColor(HelmTheme.textSecondary)
    }

    private func rustupToolchainMenuLabel(_ title: String) -> some View {
        HStack(spacing: 6) {
            Text(title)
                .font(.caption)
            Image(systemName: "chevron.down")
                .font(.caption2)
                .foregroundColor(.secondary)
        }
    }

    private func pickRustupOverrideDirectory(for package: PackageItem) {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        panel.resolvesAliases = true
        panel.prompt = L10n.Common.ok.localized
        if panel.runModal() == .OK, let url = panel.url {
            core.setRustupOverride(package, path: url.path)
        }
    }

    private var kegPolicyMenuField: some View {
        InspectorField(label: L10n.App.Packages.Label.homebrewKegPolicy.localized) {
            Menu {
                Button {
                    core.setKegPolicySelection(for: activePackage, selection: .useGlobal)
                } label: {
                    HStack(spacing: 8) {
                        Text(L10n.App.Packages.KegPolicy.useGlobal.localized)
                        if kegPolicySelection == .useGlobal {
                            Image(systemName: "checkmark")
                        }
                    }
                }

                Button {
                    core.setKegPolicySelection(for: activePackage, selection: .keep)
                } label: {
                    HStack(spacing: 8) {
                        Text(L10n.App.Packages.KegPolicy.keepOld.localized)
                        if kegPolicySelection == .keep {
                            Image(systemName: "checkmark")
                        }
                    }
                }

                Button {
                    core.setKegPolicySelection(for: activePackage, selection: .cleanup)
                } label: {
                    HStack(spacing: 8) {
                        Text(L10n.App.Packages.KegPolicy.cleanupOld.localized)
                        if kegPolicySelection == .cleanup {
                            Image(systemName: "checkmark")
                        }
                    }
                }
            } label: {
                HStack(spacing: 6) {
                    Text(kegPolicyLabel(kegPolicySelection))
                        .font(.callout)
                    Image(systemName: "chevron.down")
                        .font(.caption2)
                        .foregroundColor(.secondary)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .menuStyle(.borderlessButton)
        }
    }

    private var managerSelectionField: some View {
        InspectorField(label: L10n.App.Inspector.manager.localized) {
            VStack(alignment: .leading, spacing: 8) {
                ForEach(managerCandidates, id: \.managerId) { candidate in
                    let isActive = candidate.managerId == selectedManagerId
                    Button {
                        requestManagerSwitch(to: candidate)
                    } label: {
                        HStack(spacing: 6) {
                            Image(systemName: isActive ? "checkmark.circle.fill" : "circle")
                                .foregroundColor(isActive ? HelmTheme.proAccent : HelmTheme.textSecondary)
                                .font(.caption)
                            Text(localizedManagerDisplayName(candidate.managerId))
                                .font(.callout)
                                .foregroundColor(.primary)
                            Spacer(minLength: 0)
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .buttonStyle(.plain)
                    .disabled(isActive)
                    .helmPointer(enabled: !isActive)
                }

                if let activeManagerGroup, activeManagerGroup.packages.count > 1 {
                    Divider()
                        .padding(.vertical, 2)

                    VStack(alignment: .leading, spacing: 6) {
                        Text(L10n.App.Inspector.version.localized)
                            .font(.caption)
                            .foregroundColor(.secondary)

                        Picker(
                            L10n.App.Inspector.version.localized,
                            selection: Binding(
                                get: { activePackage.id },
                                set: { selectPackageMember(withId: $0) }
                            )
                        ) {
                            ForEach(activeManagerGroup.packages, id: \.id) { candidate in
                                Text(versionSelectionLabel(for: candidate))
                                    .tag(candidate.id)
                            }
                        }
                        .pickerStyle(.radioGroup)
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private var packageIdField: some View {
        InspectorField(label: L10n.App.Inspector.packageId.localized) {
            if managerGroups.count <= 1,
               let activeManagerGroup,
               activeManagerGroup.packages.count <= 1 {
                Text(displayedPackageIdentifier(for: activePackage))
                    .font(.caption.monospacedDigit())
                    .foregroundColor(.secondary)
            } else {
                VStack(alignment: .leading, spacing: 3) {
                    ForEach(managerGroups) { group in
                        ForEach(group.packages, id: \.id) { candidate in
                            HStack(spacing: 6) {
                                Text(packageIdLabel(for: candidate, within: group))
                                    .font(.caption)
                                    .foregroundColor(.secondary)
                                Text(displayedPackageIdentifier(for: candidate))
                                    .font(.caption.monospacedDigit())
                                    .foregroundColor(.secondary)
                            }
                        }
                    }
                }
            }
        }
    }

    private var packageActionRow: some View {
        HStack(spacing: 6) {
            if activePackage.status == .available {
                packageActionButton(
                    symbol: "arrow.down.circle",
                    tooltip: L10n.App.Packages.Action.install.localized,
                    enabled: core.canInstallPackage(activePackage, includeAlternates: false)
                        && !core.installActionPackageIds.contains(activePackage.id)
                ) {
                    core.installPackage(activePackage, includeAlternates: false)
                }
            }

            if core.canUninstallPackage(activePackage) {
                packageActionButton(
                    symbol: "trash",
                    tooltip: L10n.App.Packages.Action.uninstall.localized,
                    enabled: !core.uninstallActionPackageIds.contains(activePackage.id)
                        && !loadingPackageUninstallPreview
                ) {
                    requestPackageUninstallConfirmation(for: activePackage)
                }
            }

            if activePackage.status == .upgradable {
                packageActionButton(
                    symbol: "arrow.up.circle",
                    tooltip: L10n.App.Packages.Action.update.localized,
                    enabled: core.canUpgradeIndividually(activePackage)
                        && !core.upgradeActionPackageIds.contains(activePackage.id)
                ) {
                    core.upgradePackage(activePackage)
                }
            }

            if core.canPinPackage(activePackage) {
                if activePackage.pinned {
                    packageActionButton(
                        symbol: "pin.slash",
                        tooltip: L10n.App.Packages.Action.unpin.localized,
                        enabled: !core.pinActionPackageIds.contains(activePackage.id)
                    ) {
                        core.unpinPackage(activePackage)
                    }
                } else {
                    packageActionButton(
                        symbol: "pin",
                        tooltip: L10n.App.Packages.Action.pin.localized,
                        enabled: !core.pinActionPackageIds.contains(activePackage.id)
                    ) {
                        core.pinPackage(activePackage)
                    }
                }
            }

            packageActionButton(
                symbol: "slider.horizontal.3",
                tooltip: L10n.App.Inspector.viewManager.localized,
                enabled: true
            ) {
                context.selectedManagerId = activePackage.managerId
                context.selectedPackageId = nil
                context.selectedTaskId = nil
                context.selectedUpgradePlanStepId = nil
                context.selectedSection = .managers
            }

            Spacer(minLength: 0)
        }
    }

    private func kegPolicyLabel(_ selection: KegPolicySelection) -> String {
        switch selection {
        case .useGlobal:
            return L10n.App.Packages.KegPolicy.useGlobal.localized
        case .keep:
            return L10n.App.Packages.KegPolicy.keepOld.localized
        case .cleanup:
            return L10n.App.Packages.KegPolicy.cleanupOld.localized
        }
    }

    private func packageActionButton(
        symbol: String,
        tooltip: String,
        enabled: Bool,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
        }
        .buttonStyle(HelmIconButtonStyle())
        .help(tooltip)
        .accessibilityLabel(tooltip)
        .disabled(!enabled)
        .helmPointer(enabled: enabled)
    }

    private func requestPackageUninstallConfirmation(for targetPackage: PackageItem) {
        loadingPackageUninstallPreview = true
        core.previewPackageUninstall(targetPackage) { preview in
            loadingPackageUninstallPreview = false
            if let preview {
                inspectorAlert = .uninstall(package: targetPackage, preview: preview)
                return
            }
            inspectorAlert = .uninstallFallback(package: targetPackage)
        }
    }

    private func packageUninstallAlertMessage(_ preview: PackageUninstallPreview, package targetPackage: PackageItem) -> String {
        var sections = [
            L10n.App.Packages.Alert.uninstallMessage.localized(
                with: [
                    "package": targetPackage.displayName,
                    "manager": localizedManagerDisplayName(targetPackage.managerId),
                ]
            )
        ]

        if !preview.summaryLines.isEmpty {
            sections.append(preview.summaryLines.joined(separator: "\n"))
        }

        if !preview.secondaryEffects.isEmpty {
            let effects = preview.secondaryEffects.prefix(3).map { "• \($0)" }
            sections.append(effects.joined(separator: "\n"))
        }

        return sections.joined(separator: "\n\n")
    }

    private func requestManagerSwitch(to candidate: PackageItem) {
        let currentPackage = activePackage
        guard candidate.managerId != currentPackage.managerId else { return }

        let packageName = currentPackage.displayName
        let currentManagerName = localizedManagerDisplayName(currentPackage.managerId)
        let selectedManagerName = localizedManagerDisplayName(candidate.managerId)
        let recommendedManagerName = localizedManagerDisplayName(recommendedManagerPackage.managerId)

        var warnings = [
            "app.packages.alert.switch_manager.use_selected".localized(with: [
                "selected_manager": selectedManagerName,
                "current_manager": currentManagerName,
                "package": packageName,
            ])
        ]

        if candidate.managerId != recommendedManagerPackage.managerId {
            warnings.append(
                "app.packages.alert.switch_manager.recommended".localized(with: [
                    "recommended_manager": recommendedManagerName,
                ])
            )
        }

        if let provenancePackage = installedProvenanceManagerPackage {
            if installedManagerCandidates.count == 1,
               provenancePackage.managerId != candidate.managerId {
                let provenanceManagerName = localizedManagerDisplayName(provenancePackage.managerId)
                warnings.append(
                    "app.packages.alert.switch_manager.installed_conflict".localized(with: [
                        "package": packageName,
                        "provenance_manager": provenanceManagerName,
                        "selected_manager": selectedManagerName,
                    ])
                )
            } else if installedManagerCandidates.count > 1, candidate.status == .available {
                let installedManagers = installedManagerCandidates
                    .map { localizedManagerDisplayName($0.managerId) }
                    .joined(separator: ", ")
                warnings.append(
                    "app.packages.alert.switch_manager.multi_installed_conflict".localized(with: [
                        "package": packageName,
                        "installed_managers": installedManagers,
                        "selected_manager": selectedManagerName,
                    ])
                )
            }
        }

        if candidate.status == .available,
           !core.canInstallPackage(candidate, includeAlternates: false) {
            warnings.append(
                "app.packages.alert.switch_manager.unavailable_install".localized(with: [
                    "selected_manager": selectedManagerName,
                ])
            )
        }

        inspectorAlert = .switchManager(
            ManagerSwitchAlertContext(
                packageName: packageName,
                selectedManagerId: candidate.managerId,
                title: "app.packages.alert.switch_manager.title".localized,
                message: warnings.joined(separator: "\n\n")
            )
        )
    }

    private func persistManagerSelection(_ managerId: String) {
        context.selectedManagerId = managerId
        if let preferredPackage = preferredPackage(forManagerId: managerId) {
            context.selectedPackageId = preferredPackage.id
        }
        core.setPreferredManagerId(managerId, for: livePackage)
    }

    private func ensurePersistedManagerSelection() {
        if let preferredManagerId = core.preferredManagerId(for: livePackage),
           managerCandidates.contains(where: { $0.managerId == preferredManagerId }) {
            return
        }
        persistManagerSelection(recommendedManagerPackage.managerId)
    }

    private func ensureSelectedPackageMember() {
        if let activeManagerGroup,
           let selectedPackageId = context.selectedPackageId,
           activeManagerGroup.packages.contains(where: { $0.id == selectedPackageId }) {
            return
        }
        if let preferredPackage = preferredPackage(forManagerId: selectedManagerId) {
            context.selectedPackageId = preferredPackage.id
        }
    }

    private func selectPackageMember(withId packageId: String) {
        guard let activeManagerGroup,
              activeManagerGroup.packages.contains(where: { $0.id == packageId }) else {
            return
        }
        context.selectedManagerId = selectedManagerId
        context.selectedPackageId = packageId
    }

    private func preferredPackage(forManagerId managerId: String) -> PackageItem? {
        activePackageGroup(forManagerId: managerId)?.representativePackage
    }

    private func activePackageGroup(forManagerId managerId: String) -> ManagerPackageGroup? {
        managerGroups.first(where: { $0.managerId == managerId })
    }

    private func packageFamilyMembers(for package: PackageItem) -> [PackageItem] {
        core.packageFamilyCandidates(for: package)
            .filter { core.isManagerEnabled($0.managerId) }
            .sorted { lhs, rhs in
            if lhs.managerId == rhs.managerId {
                return memberCandidateOrdering(lhs, rhs)
            }
            return managerCandidateOrdering(lhs, rhs)
        }
    }

    private func preferredCandidate(_ lhs: PackageItem, _ rhs: PackageItem) -> PackageItem {
        let lhsRank = packageStatusSortRank(lhs.status)
        let rhsRank = packageStatusSortRank(rhs.status)
        if lhsRank != rhsRank {
            return lhsRank < rhsRank ? lhs : rhs
        }

        let lhsVersionKnown = normalizedVersionText(lhs.version) != nil
        let rhsVersionKnown = normalizedVersionText(rhs.version) != nil
        if lhsVersionKnown != rhsVersionKnown {
            return lhsVersionKnown ? lhs : rhs
        }

        let lhsVersion = normalizedVersionText(lhs.version)
        let rhsVersion = normalizedVersionText(rhs.version)
        if let lhsVersion, let rhsVersion {
            let order = lhsVersion.compare(rhsVersion, options: [.numeric, .caseInsensitive])
            if order != .orderedSame {
                return order == .orderedDescending ? lhs : rhs
            }
        }

        let lhsSummary = lhs.summary?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        let rhsSummary = rhs.summary?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        if lhsSummary.isEmpty != rhsSummary.isEmpty {
            return lhsSummary.isEmpty ? rhs : lhs
        }

        let lhsId = lhs.id.lowercased()
        let rhsId = rhs.id.lowercased()
        if lhsId != rhsId {
            return lhsId < rhsId ? lhs : rhs
        }

        return lhs
    }

    private func memberCandidateOrdering(_ lhs: PackageItem, _ rhs: PackageItem) -> Bool {
        preferredCandidate(lhs, rhs).id == lhs.id
    }

    private func managerCandidateOrdering(_ lhs: PackageItem, _ rhs: PackageItem) -> Bool {
        let lhsPriority = core.managerPriorityRank(for: lhs.managerId)
        let rhsPriority = core.managerPriorityRank(for: rhs.managerId)
        if lhsPriority != rhsPriority {
            return lhsPriority < rhsPriority
        }
        return localizedManagerDisplayName(lhs.managerId)
            .localizedCaseInsensitiveCompare(localizedManagerDisplayName(rhs.managerId)) == .orderedAscending
    }

    private func packageStatusSortRank(_ status: PackageStatus) -> Int {
        switch status {
        case .upgradable:
            return 0
        case .installed:
            return 1
        case .available:
            return 2
        }
    }

    private func normalizedVersionText(_ value: String?) -> String? {
        guard let value else { return nil }
        let normalized = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !normalized.isEmpty else { return nil }
        if Self.unknownVersionTokens.contains(normalized.lowercased()) {
            return nil
        }
        return normalized
    }

    private func versionSelectionLabel(for package: PackageItem) -> String {
        let versionText = normalizedVersionText(package.version) ?? package.displayName
        var flags: [String] = []
        if package.runtimeState.isActive {
            flags.append(L10n.App.Inspector.packageRuntimeStateActive.localized)
        }
        if package.runtimeState.isDefault {
            flags.append(L10n.App.Inspector.packageRuntimeStateDefault.localized)
        }
        if package.runtimeState.hasOverride {
            flags.append(L10n.App.Inspector.packageRuntimeStateOverride.localized)
        }
        if package.status == .upgradable, let latestVersion = normalizedVersionText(package.latestVersion) {
            flags.append(L10n.App.Inspector.latest.localized + " " + latestVersion)
        }
        guard !flags.isEmpty else { return versionText }
        return versionText + " (" + flags.joined(separator: ", ") + ")"
    }

    private func packageIdLabel(for package: PackageItem, within group: ManagerPackageGroup) -> String {
        let managerName = localizedManagerDisplayName(group.managerId)
        guard group.packages.count > 1, let versionText = normalizedVersionText(package.version) else {
            return managerName
        }
        return managerName + " " + versionText
    }

    private func displayedPackageIdentifier(for package: PackageItem) -> String {
        let trimmedIdentifier = package.packageIdentifier?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        if !trimmedIdentifier.isEmpty {
            return trimmedIdentifier
        }
        return package.id
    }
}

private extension InspectorManagerDetailView {
    var genericManagerPackageStateIssues: [ManagerPackageStateIssue] {
        (status?.packageStateIssues ?? []).filter { issue in
            issue.issueCode != "metadata_only_install"
                && issue.issueCode != "post_install_setup_required"
        }
    }

    @ViewBuilder
    func genericPackageStateIssueBanner(_ issue: ManagerPackageStateIssue) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .top, spacing: 8) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundColor(HelmTheme.stateAttention)
                    .padding(.top, 2)
                VStack(alignment: .leading, spacing: 4) {
                    Text(issue.summary ?? issue.issueCode)
                        .font(.callout.weight(.semibold))
                        .fixedSize(horizontal: false, vertical: true)

                    if let evidencePrimary = issue.evidencePrimary,
                       !evidencePrimary.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                        Text(evidencePrimary)
                            .font(.caption)
                            .foregroundColor(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }

                    if let evidenceSecondary = issue.evidenceSecondary,
                       !evidenceSecondary.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                        Text(evidenceSecondary)
                            .font(.caption2)
                            .foregroundColor(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                Spacer(minLength: 0)
            }

            if let repairOptions = issue.repairOptions, !repairOptions.isEmpty {
                HStack(spacing: 8) {
                    ForEach(repairOptions, id: \.optionId) { option in
                        Button(option.title) {
                            core.applyManagerPackageStateIssueRepair(
                                managerId: manager.id,
                                sourceManagerId: issue.sourceManagerId,
                                packageName: issue.packageName,
                                issueCode: issue.issueCode,
                                optionId: option.optionId
                            )
                        }
                        .buttonStyle(HelmSecondaryButtonStyle())
                        .disabled(managerIsUninstalling)
                        .helmPointer(enabled: !managerIsUninstalling)
                    }
                    Spacer(minLength: 0)
                }
            }
        }
        .padding(10)
        .background(
            RoundedRectangle(cornerRadius: 10)
                .fill(HelmTheme.surfaceElevated)
        )
    }
}

private struct InspectorAttributedText: NSViewRepresentable {
    let attributedText: NSAttributedString

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeNSView(context: Context) -> InspectorLinkTextView {
        let textView = InspectorLinkTextView()
        textView.delegate = context.coordinator
        textView.isEditable = false
        textView.isSelectable = true
        textView.drawsBackground = false
        textView.isRichText = true
        textView.isHorizontallyResizable = false
        textView.isVerticallyResizable = true
        textView.textContainerInset = .zero
        textView.textContainer?.lineFragmentPadding = 0
        textView.textContainer?.widthTracksTextView = true
        textView.textContainer?.heightTracksTextView = false
        textView.linkTextAttributes = [
            .foregroundColor: NSColor.linkColor,
            .underlineStyle: NSUnderlineStyle.single.rawValue
        ]
        return textView
    }

    func updateNSView(_ nsView: InspectorLinkTextView, context: Context) {
        nsView.textStorage?.setAttributedString(attributedText)
        nsView.invalidateIntrinsicContentSize()
    }

    final class Coordinator: NSObject, NSTextViewDelegate {
        func textView(_ textView: NSTextView, clickedOnLink link: Any, at charIndex: Int) -> Bool {
            guard let url = InspectorLinkPolicy.safeURL(from: link) else { return false }
            NSWorkspace.shared.open(url)
            return true
        }
    }
}

private final class InspectorLinkTextView: NSTextView {
    override var intrinsicContentSize: NSSize {
        guard let textContainer, let layoutManager else {
            return NSSize(width: NSView.noIntrinsicMetric, height: 0)
        }

        let fittingWidth = bounds.width > 0 ? bounds.width : 320
        if textContainer.containerSize.width != fittingWidth {
            textContainer.containerSize = NSSize(width: fittingWidth, height: .greatestFiniteMagnitude)
        }

        layoutManager.ensureLayout(for: textContainer)
        let usedRect = layoutManager.usedRect(for: textContainer)
        let height = ceil(usedRect.height + (textContainerInset.height * 2))
        return NSSize(width: NSView.noIntrinsicMetric, height: max(height, 14))
    }

    override func layout() {
        super.layout()
        invalidateIntrinsicContentSize()
    }
}

// MARK: - Manager Inspector

private struct InspectorManagerDetailView: View {
    @ObservedObject private var core = HelmCore.shared
    @EnvironmentObject private var context: ControlCenterContext
    private let installMethodPolicyContext = ManagerInstallMethodPolicyContext.fromEnvironment()
    let manager: ManagerInfo
    let status: ManagerStatus?
    let detectionDiagnostics: ManagerDetectionDiagnostics
    let health: OperationalHealth
    let packageCount: Int
    let outdatedCount: Int
    @State private var confirmAction: ConfirmAction?
    @State private var uninstallConfirmation: UninstallConfirmationContext?
    @State private var showUninstallDetails = false
    @State private var loadingManagerUninstallPreview = false
    @State private var showInstallOptionsSheet = false
    @State private var pendingInstallMethodRawValue: String?
    @State private var pendingInstallMethodOptions: [ManagerInstallMethodOption] = []
    @State private var pendingHardTimeoutSeconds: Int?
    @State private var pendingIdleTimeoutSeconds: Int?
    @State private var showAdvancedInstallOptions = false
    @State private var installSubmissionInFlight = false
    @State private var showPackageStateIssueDetails = false
    @State private var pendingRustupInstallSource: ManagerRustupInstallSource = .officialDownload
    @State private var pendingRustupBinaryPath = ""
    @State private var pendingMiseInstallSource: ManagerMiseInstallSource = .officialDownload
    @State private var pendingMiseBinaryPath = ""
    @State private var pendingCompletePostInstallSetupAutomatically = false
    @State private var showPostInstallSetupSheet = false
    @State private var verifyingPostInstallSetup = false
    @State private var postInstallSetupVerificationMessage: String?
    @State private var activeInstanceUpdateInFlightId: String?
    @State private var expandedInstallInstanceIds: Set<String> = []
    @State private var pendingUninstallOptions = ManagerUninstallActionOptions(
        allowUnknownProvenance: false,
        homebrewCleanupMode: nil,
        miseCleanupMode: nil,
        miseConfigRemoval: nil,
        removeHelmManagedShellSetup: nil
    )

    private enum ConfirmAction: Identifiable {
        case update
        case enableRequiredManagerForInstance(
            parentManagerId: String,
            instanceId: String,
            followUp: ManagedInstanceFollowUpAction
        )

        var id: String {
            switch self {
            case .update:
                return "update"
            case let .enableRequiredManagerForInstance(parentManagerId, instanceId, followUp):
                return "enable-required-\(parentManagerId)-\(instanceId)-\(followUp.id)"
            }
        }
    }

    private enum ManagedInstanceFollowUpAction {
        case none
        case update
        case uninstall(allowUnknownProvenance: Bool)

        var id: String {
            switch self {
            case .none:
                return "none"
            case .update:
                return "update"
            case let .uninstall(allowUnknownProvenance):
                return "uninstall-\(allowUnknownProvenance)"
            }
        }
    }

    private struct UninstallConfirmationContext: Identifiable {
        let preview: ManagerUninstallPreview?
        let allowUnknownProvenance: Bool

        var id: String {
            if let preview {
                return "preview-\(preview.strategy)-\(preview.blastRadiusScore)-\(allowUnknownProvenance)"
            }
            return "fallback-\(allowUnknownProvenance)"
        }

        var readOnlyBlocked: Bool {
            preview?.readOnlyBlocked ?? false
        }

        var resolvedAllowUnknownProvenance: Bool {
            allowUnknownProvenance || (preview?.unknownOverrideRequired ?? false)
        }
    }

    private var detected: Bool {
        core.isManagerDetected(manager.id)
    }

    private var enabled: Bool {
        status?.enabled ?? true
    }

    private var activeExecutablePath: String? {
        status?.executablePath?.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var defaultExecutablePath: String? {
        let fromStatus = status?.defaultExecutablePath?.trimmingCharacters(in: .whitespacesAndNewlines)
        if let fromStatus, !fromStatus.isEmpty {
            return fromStatus
        }
        return executablePaths.first
    }

    private var selectedExecutablePath: String? {
        let fromStatus = status?.selectedExecutablePath?.trimmingCharacters(in: .whitespacesAndNewlines)
        if let fromStatus, !fromStatus.isEmpty {
            return fromStatus
        }
        return defaultExecutablePath ?? activeExecutablePath
    }

    private var executablePaths: [String] {
        var paths: [String] = []
        let discoveredPaths = status?.executablePaths ?? []
        for path in discoveredPaths {
            let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
            if !trimmed.isEmpty, !paths.contains(trimmed) {
                paths.append(trimmed)
            }
        }
        if let activeExecutablePath, !activeExecutablePath.isEmpty, !paths.contains(activeExecutablePath) {
            paths.insert(activeExecutablePath, at: 0)
        }
        return paths
    }

    private var recommendedExecutablePath: String? {
        manager.recommendedExecutablePath(
            from: executablePaths,
            methodOptions: resolvedInstallMethodOptions
        ) ?? defaultExecutablePath
    }

    private var selectedInstallMethodOption: ManagerInstallMethodOption {
        manager.selectedInstallMethodOption(
            selectedMethodRawValue: status?.selectedInstallMethod,
            executablePath: selectedExecutablePath ?? activeExecutablePath,
            installedPackages: core.installedPackages,
            methodOptions: resolvedInstallMethodOptions
        )
    }

    private var resolvedInstallMethodOptions: [ManagerInstallMethodOption] {
        guard let coreOptions = status?.installMethodOptions,
              !coreOptions.isEmpty else {
            return manager.installMethodOptions
        }

        let fallbackByMethod = Dictionary(
            uniqueKeysWithValues: manager.installMethodOptions.map { ($0.method.rawValue, $0) }
        )
        let mapped = coreOptions.compactMap { option in
            ManagerInstallMethodOption.fromCoreStatus(
                option,
                fallback: fallbackByMethod[option.methodId]
            )
        }
        return mapped.isEmpty ? manager.installMethodOptions : mapped
    }

    private var helmSupportedInstallMethodRawValues: Set<String> {
        Set(resolvedInstallMethodOptions.map(\.method.rawValue))
    }

    private var sortedHelmSupportedInstallMethodOptions: [ManagerInstallMethodOption] {
        resolvedInstallMethodOptions
            .filter { helmSupportedInstallMethodRawValues.contains($0.method.rawValue) }
            .sorted { lhs, rhs in
                let lhsRank = lhs.recommendationRank
                let rhsRank = rhs.recommendationRank
                if lhsRank != rhsRank {
                    return lhsRank < rhsRank
                }
                return localizedInstallMethod(lhs.method)
                    .localizedCaseInsensitiveCompare(localizedInstallMethod(rhs.method)) == .orderedAscending
            }
    }

    private var selectedPendingInstallMethodIsAllowed: Bool {
        guard let option = selectedPendingInstallMethodOption else {
            return false
        }
        return installMethodOptionAllowed(option)
    }

    private var selectedPendingInstallMethodOption: ManagerInstallMethodOption? {
        guard let pendingInstallMethodRawValue else { return nil }
        return pendingInstallMethodOptions.first(where: {
            $0.method.rawValue == pendingInstallMethodRawValue
        })
    }

    private var selectedInstallMethodUnavailableMessage: String? {
        guard let option = selectedPendingInstallMethodOption else { return nil }
        guard !installMethodOptionAllowed(option) else { return nil }
        guard let dependencyManagerId = ManagerDependencyResolver.dependencyManagerId(
            for: manager.id,
            installMethod: option.method
        ) else {
            return nil
        }

        let methodName = localizedInstallMethod(option.method)
        let dependencyName = localizedManagerDisplayName(dependencyManagerId)
        if !core.isManagerDetected(dependencyManagerId) {
            return "\(methodName): \(dependencyName) (\(L10n.Common.notInstalled.localized))"
        }

        let dependencyEnabled = core.managerStatuses[dependencyManagerId]?.enabled ?? false
        if !dependencyEnabled {
            return "\(methodName): \(dependencyName) (\(L10n.Common.disabled.localized))"
        }

        return nil
    }

    private var rustupInstallMethodSelected: Bool {
        pendingInstallMethodRawValue == ManagerDistributionMethod.rustupInstaller.rawValue
    }

    private var miseScriptInstallMethodSelected: Bool {
        manager.id == "mise"
            && pendingInstallMethodRawValue == ManagerDistributionMethod.scriptInstaller.rawValue
    }

    private var rustupInstallSourceRequiresBinaryPath: Bool {
        rustupInstallMethodSelected && pendingRustupInstallSource == .existingBinaryPath
    }

    private var miseInstallSourceRequiresBinaryPath: Bool {
        miseScriptInstallMethodSelected && pendingMiseInstallSource == .existingBinaryPath
    }

    private var rustupInstallSourceSelectionValid: Bool {
        guard rustupInstallSourceRequiresBinaryPath else {
            return true
        }
        return !pendingRustupBinaryPath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private var miseInstallSourceSelectionValid: Bool {
        guard miseInstallSourceRequiresBinaryPath else {
            return true
        }
        return !pendingMiseBinaryPath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private var miseUninstallCleanupModeSelection: ManagerMiseUninstallCleanupMode {
        pendingUninstallOptions.miseCleanupMode ?? .managerOnly
    }

    private var homebrewUninstallCleanupModeSelection: ManagerHomebrewUninstallCleanupMode {
        pendingUninstallOptions.homebrewCleanupMode ?? .managerOnly
    }

    private var miseRequiresConfigRemovalSelection: Bool {
        manager.id == "mise" && miseUninstallCleanupModeSelection == .fullCleanup
    }

    private var uninstallConfigSelectionValid: Bool {
        if !miseRequiresConfigRemovalSelection {
            return true
        }
        return pendingUninstallOptions.miseConfigRemoval != nil
    }

    private var hardTimeoutOptions: [Int?] {
        [nil, 120, 300, 600, 900, 1200, 1800]
    }

    private var idleTimeoutOptions: [Int?] {
        [nil, 30, 60, 90, 120, 180, 300, 600]
    }

    private var latestFailedTask: TaskItem? {
        core.activeTasks.first { task in
            task.status.lowercased() == "failed" && task.managerId == manager.id
        }
    }

    private var installInstanceCount: Int {
        status?.installInstanceCount ?? 0
    }

    private var installInstances: [ManagerInstallInstanceStatus] {
        let instances = status?.installInstances ?? []
        return instances.sorted { left, right in
            let leftBucket = installInstanceRecommendationBucket(for: left)
            let rightBucket = installInstanceRecommendationBucket(for: right)
            if leftBucket != rightBucket {
                return leftBucket < rightBucket
            }

            let leftDependencyRank = dependencyManagerRecommendationRank(for: left)
            let rightDependencyRank = dependencyManagerRecommendationRank(for: right)
            if leftDependencyRank != rightDependencyRank {
                return leftDependencyRank < rightDependencyRank
            }

            let leftClassRank = nonDependencyRecommendationRank(for: left)
            let rightClassRank = nonDependencyRecommendationRank(for: right)
            if leftClassRank != rightClassRank {
                return leftClassRank < rightClassRank
            }

            if left.confidence != right.confidence {
                return left.confidence > right.confidence
            }

            if left.isActive != right.isActive {
                return left.isActive && !right.isActive
            }

            return left.displayPath.localizedStandardCompare(right.displayPath) == .orderedAscending
        }
    }

    private var installedManagerRecommendationOrder: [String: Int] {
        let ordered = core.managersState.authoritativeManagers
            + core.managersState.standardManagers
            + core.managersState.guardedManagers
        var mapping: [String: Int] = [:]
        var cursor = 0
        for managerInfo in ordered where core.isManagerDetected(managerInfo.id) {
            guard mapping[managerInfo.id] == nil else { continue }
            mapping[managerInfo.id] = cursor
            cursor += 1
        }
        return mapping
    }

    // Business follow-up: wire designated install/provenance method ordering when Helm Business policy is implemented.
    private var businessDesignatedProvenance: String? {
        nil
    }

    private func installInstanceRecommendationBucket(for instance: ManagerInstallInstanceStatus) -> Int {
        let provenance = normalizedProvenance(instance.provenance)

        if let designated = businessDesignatedProvenance,
           provenance == normalizedProvenance(designated) {
            return 0
        }
        if isOfficialDirectProvenance(provenance) {
            return 1
        }
        if requiresManagerDependency(provenance) {
            return 3
        }
        if provenance == "unknown" {
            return 4
        }
        return 2
    }

    private func dependencyManagerRecommendationRank(
        for instance: ManagerInstallInstanceStatus
    ) -> Int {
        guard let managerId = provenanceDependencyManagerId(instance.provenance) else {
            return Int.max / 2
        }
        if let rank = installedManagerRecommendationOrder[managerId] {
            return rank
        }
        return core.managerPriorityRank(for: managerId)
    }

    private func nonDependencyRecommendationRank(
        for instance: ManagerInstallInstanceStatus
    ) -> Int {
        switch normalizedProvenance(instance.provenance) {
        case "rustup_init": return 0
        case "mise": return manager.id == "mise" ? 1 : 6
        case "source_build": return 2
        case "system": return 3
        case "enterprise_managed": return 4
        case "unknown": return 99
        default: return 10
        }
    }

    private func normalizedProvenance(_ value: String) -> String {
        value.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
    }

    private func isOfficialDirectProvenance(_ provenance: String) -> Bool {
        if provenance == "source_build" {
            return true
        }
        if manager.id == "rustup" && provenance == "rustup_init" {
            return true
        }
        if manager.id == "mise" && provenance == "mise" {
            return true
        }
        return false
    }

    private func requiresManagerDependency(_ provenance: String) -> Bool {
        ManagerDependencyResolver.dependencyManagerId(for: manager.id, provenance: provenance) != nil
    }

    private func provenanceDependencyManagerId(_ provenance: String) -> String? {
        ManagerDependencyResolver.dependencyManagerId(for: manager.id, provenance: provenance)
    }

    private var multiInstanceState: String {
        status?.multiInstanceState?.trimmingCharacters(in: .whitespacesAndNewlines) ?? "none"
    }

    private var multiInstanceAttentionNeeded: Bool {
        multiInstanceState == "attention_needed" && installInstanceCount > 1
    }

    private var multiInstanceAcknowledged: Bool {
        multiInstanceState == "acknowledged" && installInstanceCount > 1
    }

    private func competingProvenanceSummary(
        for instance: ManagerInstallInstanceStatus
    ) -> String? {
        guard let provenance = instance.competingProvenance?.trimmingCharacters(in: .whitespacesAndNewlines),
              !provenance.isEmpty else {
            return nil
        }
        if let confidence = instance.competingConfidence {
            return "\(provenance) (\(formatConfidence(confidence)))"
        }
        return provenance
    }

    private var managerHealthIsError: Bool {
        if case .error = health {
            return true
        }
        return false
    }

    private var managerIsUninstalling: Bool {
        core.isManagerUninstalling(manager.id)
    }

    private var metadataOnlyPackageStateIssue: ManagerPackageStateIssue? {
        (status?.packageStateIssues ?? []).first(where: { issue in
            issue.issueCode == "metadata_only_install"
        })
    }

    private var postInstallSetupIssue: ManagerPackageStateIssue? {
        guard !postInstallSetupTaskInFlight else { return nil }
        return (status?.packageStateIssues ?? []).first(where: { issue in
            issue.issueCode == "post_install_setup_required"
        })
    }

    private var postInstallSetupTaskInFlight: Bool {
        core.activeTasks.contains { task in
            task.managerId == manager.id
                && task.labelKey == "service.task.label.setup.manager"
                && task.isRunning
        }
    }

    private var supportsPostInstallSetupAutomation: Bool {
        manager.id == "rustup" || manager.id == "mise" || manager.id == "asdf"
    }

    private var managerCanInstall: Bool {
        let supportsHelmInstall = Set([
            "mise",
            "asdf",
            "mas",
            "rustup",
            "npm",
            "pnpm",
            "yarn",
            "pipx",
            "pip",
            "poetry",
            "rubygems",
            "bundler",
            "cargo",
            "cargo_binstall",
            "podman",
            "colima"
        ]).contains(manager.id)
        guard supportsHelmInstall else { return false }
        let allowedOptions = sortedHelmSupportedInstallMethodOptions.filter { option in
            option.method != .notManageable && option.isAllowed(in: installMethodPolicyContext)
        }
        if !allowedOptions.isEmpty {
            return true
        }
        return manager.canInstall
    }

    private var supportsShellSetupTeardownOption: Bool {
        manager.id == "rustup" || manager.id == "mise" || manager.id == "asdf"
    }

    private var defaultShellSetupTeardownSelection: Bool {
        supportsShellSetupTeardownOption && installInstanceCount <= 1
    }

    private var shellSetupTeardownSelection: Bool {
        guard supportsShellSetupTeardownOption else { return false }
        return pendingUninstallOptions.removeHelmManagedShellSetup ?? defaultShellSetupTeardownSelection
    }

    private var postInstallSetupAutomationAvailable: Bool {
        postInstallSetupIssueSupportsRepairOption("apply_post_install_setup_defaults")
    }

    private func postInstallSetupIssueSupportsRepairOption(_ optionId: String) -> Bool {
        guard let issue = postInstallSetupIssue else { return false }
        guard let options = issue.repairOptions, !options.isEmpty else { return false }
        return options.contains { option in
            option.optionId.caseInsensitiveCompare(optionId) == .orderedSame
        }
    }

    private var metadataOnlyIssueInstalledPackage: PackageItem? {
        guard let issue = metadataOnlyPackageStateIssue else { return nil }
        return core.installedPackages.first(where: { package in
            package.managerId == issue.sourceManagerId
                && package.name.caseInsensitiveCompare(issue.packageName) == .orderedSame
        })
    }

    private var metadataOnlyIssueCanRemoveStaleEntry: Bool {
        guard let package = metadataOnlyIssueInstalledPackage else {
            return false
        }
        return core.canUninstallPackage(package)
            && metadataOnlyIssueSupportsRepairOption("remove_stale_package_entry")
    }

    private var metadataOnlyIssueCanRepairInstall: Bool {
        metadataOnlyIssueSupportsRepairOption("reinstall_manager_via_homebrew")
    }

    private func metadataOnlyIssueSupportsRepairOption(_ optionId: String) -> Bool {
        guard let issue = metadataOnlyPackageStateIssue else { return false }
        guard let options = issue.repairOptions, !options.isEmpty else { return false }
        return options.contains { option in
            option.optionId.caseInsensitiveCompare(optionId) == .orderedSame
        }
    }

    private func metadataOnlyIssueExpectedPaths(for issue: ManagerPackageStateIssue) -> [String] {
        let packageName = issue.packageName.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !packageName.isEmpty else {
            return []
        }
        return [
            "/opt/homebrew/opt/\(packageName)/bin/\(packageName)",
            "/usr/local/opt/\(packageName)/bin/\(packageName)"
        ]
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 8) {
                Image(systemName: manager.symbolName)
                    .foregroundColor(.secondary)
                VStack(alignment: .leading, spacing: 2) {
                    Text(localizedManagerDisplayName(manager.id))
                        .font(.title3.weight(.semibold))
                    if let version = status?.version {
                        Text(version)
                            .font(.caption.monospacedDigit())
                            .foregroundColor(.secondary)
                    }
                }
                Spacer()
                HealthBadgeView(status: health)
            }

            HStack(spacing: 6) {
                Text(L10n.App.Managers.Label.packageCount.localized(with: ["count": packageCount]))
                    .font(.caption)
                    .foregroundColor(.secondary)
                Text("|")
                    .font(.caption)
                    .foregroundColor(.secondary)
                Text(L10n.App.Managers.Tooltip.outdated.localized(with: ["count": outdatedCount]))
                    .font(.caption)
                    .foregroundColor(outdatedCount == 0 ? HelmTheme.textSecondary : HelmTheme.stateAttention)
            }

            if multiInstanceAttentionNeeded {
                multiInstanceBanner(
                    icon: "exclamationmark.triangle.fill",
                    tint: HelmTheme.stateAttention,
                    title: L10n.App.Inspector.MultiInstance.attentionTitle.localized,
                    message: L10n.App.Inspector.MultiInstance.attentionMessage.localized
                ) {
                    Button(L10n.App.Inspector.MultiInstance.keepMultiple.localized) {
                        core.acknowledgeManagerMultiInstanceState(manager.id)
                    }
                    .buttonStyle(HelmSecondaryButtonStyle())
                    .disabled(managerIsUninstalling)
                    .helmPointer(enabled: !managerIsUninstalling)
                }
            } else if multiInstanceAcknowledged {
                multiInstanceBanner(
                    icon: "checkmark.seal.fill",
                    tint: HelmTheme.stateHealthy,
                    title: L10n.App.Inspector.MultiInstance.acknowledgedTitle.localized,
                    message: L10n.App.Inspector.MultiInstance.acknowledgedMessage.localized
                ) {
                    Button(L10n.App.Inspector.MultiInstance.reevaluate.localized) {
                        core.clearManagerMultiInstanceAck(manager.id)
                    }
                    .buttonStyle(HelmSecondaryButtonStyle())
                    .disabled(managerIsUninstalling)
                    .helmPointer(enabled: !managerIsUninstalling)
                }
            }

            if let issue = metadataOnlyPackageStateIssue {
                packageStateIssueBanner(issue)
            }

            if let issue = postInstallSetupIssue {
                postInstallSetupBanner(issue)
            }

            ForEach(Array(genericManagerPackageStateIssues.enumerated()), id: \.offset) { _, issue in
                genericPackageStateIssueBanner(issue)
            }

            InspectorField(label: L10n.App.Inspector.category.localized) {
                Text(localizedCategoryName(manager.category))
                    .font(.callout)
            }

            Text(manager.authority.key.localized)
                .font(.callout)
                .foregroundColor(.secondary)

            Group {
                HStack(alignment: .top, spacing: 6) {
                    Image(systemName: detectionDiagnosticsIconName(detectionDiagnostics.reason))
                        .foregroundColor(detectionDiagnosticsIconColor(detectionDiagnostics.reason))
                        .padding(.top, 1)
                    Text(localizedDetectionReason(detectionDiagnostics.reason))
                        .font(.callout)
                }
                .accessibilityElement(children: .combine)
                .accessibilityValue(detected
                    ? L10n.App.Inspector.detected.localized
                    : L10n.App.Inspector.notDetected.localized)

                if installInstanceCount > 0 {
                    InspectorField(label: L10n.App.Inspector.installInstanceCount.localized) {
                        VStack(alignment: .leading, spacing: 8) {
                            ForEach(installInstances) { instance in
                                let switchingManagedInstance =
                                    activeInstanceUpdateInFlightId == instance.instanceId
                                let anyInstanceSwitchInFlight = activeInstanceUpdateInFlightId != nil
                                let isExpanded = expandedInstallInstanceIds.contains(instance.instanceId)

                                VStack(alignment: .leading, spacing: 8) {
                                    Button {
                                        toggleInstallInstanceExpansion(instance.instanceId)
                                    } label: {
                                        HStack(alignment: .top, spacing: 6) {
                                            if instance.isActive {
                                                Image(systemName: "checkmark.circle.fill")
                                                    .foregroundColor(HelmTheme.stateHealthy)
                                                    .padding(.top, 1)
                                            }
                                            Text(instance.displayPath)
                                                .font(.system(.caption, design: .monospaced))
                                                .lineLimit(2)
                                                .multilineTextAlignment(.leading)
                                            Spacer(minLength: 0)
                                            Image(systemName: isExpanded ? "chevron.down" : "chevron.right")
                                                .font(.caption2.weight(.semibold))
                                                .foregroundColor(.secondary)
                                                .padding(.top, 2)
                                        }
                                    }
                                    .buttonStyle(.plain)
                                    .helmPointer()

                                    if isExpanded {
                                        VStack(alignment: .leading, spacing: 4) {
                                            Text("\(L10n.App.Inspector.provenance.localized): \(instance.provenance)")
                                                .font(.caption)
                                                .foregroundColor(.secondary)
                                            Text("\(L10n.App.Inspector.confidence.localized): \(formatConfidence(instance.confidence))")
                                                .font(.caption.monospacedDigit())
                                                .foregroundColor(.secondary)
                                            if let decisionMargin = instance.decisionMargin {
                                                Text("\(L10n.App.Inspector.decisionMargin.localized): \(formatConfidence(decisionMargin))")
                                                    .font(.caption.monospacedDigit())
                                                    .foregroundColor(.secondary)
                                            }
                                            if let explanation = instance.explanationPrimary?.trimmingCharacters(in: .whitespacesAndNewlines),
                                               !explanation.isEmpty {
                                                Text("\(L10n.App.Inspector.explanation.localized): \(explanation)")
                                                    .font(.caption)
                                                    .foregroundColor(.secondary)
                                                    .lineLimit(3)
                                            }
                                            if let competing = competingProvenanceSummary(for: instance) {
                                                Text("\(L10n.App.Inspector.competingProvenance.localized): \(competing)")
                                                    .font(.caption)
                                                    .foregroundColor(.secondary)
                                            }
                                        }
                                    }

                                    HStack(spacing: 6) {
                                        if !instance.isActive {
                                            managerActionButton(
                                                symbol: "scope",
                                                tooltip: L10n.App.Inspector.MultiInstance.manageInstance.localized,
                                                enabled: !managerIsUninstalling && !anyInstanceSwitchInFlight
                                            ) {
                                                performWithManagedInstance(instance, followUp: .none)
                                            }
                                        }

                                        if manager.canUpdate && detected && enabled {
                                            managerActionButton(
                                                symbol: "arrow.up.circle",
                                                tooltip: L10n.Common.update.localized,
                                                enabled: !managerIsUninstalling && !anyInstanceSwitchInFlight
                                            ) {
                                                performWithManagedInstance(instance, followUp: .update)
                                            }
                                        }

                                        if manager.canUninstall && detected {
                                            managerActionButton(
                                                symbol: "trash",
                                                tooltip: L10n.Common.uninstall.localized,
                                                enabled: !loadingManagerUninstallPreview
                                                    && !managerIsUninstalling
                                                    && !anyInstanceSwitchInFlight
                                            ) {
                                                performWithManagedInstance(
                                                    instance,
                                                    followUp: .uninstall(allowUnknownProvenance: false)
                                                )
                                            }
                                        }

                                        Spacer(minLength: 0)
                                        if switchingManagedInstance {
                                            ProgressView()
                                                .controlSize(.small)
                                        }
                                    }
                                }
                                .padding(8)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .background(
                                    RoundedRectangle(cornerRadius: 8)
                                        .fill(HelmTheme.surfaceElevated)
                                )
                            }
                        }
                    }
                }

            }

            Group {
                if managerHealthIsError, let failedTask = latestFailedTask {
                    Button(L10n.App.Inspector.viewDiagnostics.localized) {
                        context.selectedTaskId = failedTask.id
                        context.selectedPackageId = nil
                        context.selectedUpgradePlanStepId = nil
                    }
                    .font(.caption)
                    .buttonStyle(HelmSecondaryButtonStyle())
                    .helmPointer()
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .sheet(isPresented: $showInstallOptionsSheet) {
            VStack(alignment: .leading, spacing: 16) {
                Text(
                    L10n.App.Managers.Alert.installTitle.localized(
                        with: ["manager": localizedManagerDisplayName(manager.id)]
                    )
                )
                .font(.title3.weight(.semibold))

                Text(
                    L10n.App.Managers.Alert.installMessage.localized(
                        with: [
                            "manager_short": manager.shortName,
                            "method": pendingInstallMethodDisplayName
                        ]
                    )
                )
                .font(.callout)
                .foregroundColor(.secondary)

                VStack(alignment: .leading, spacing: 8) {
                    Text(L10n.App.Inspector.installMethod.localized)
                        .font(.caption.weight(.semibold))
                        .foregroundColor(.secondary)
                    Picker(
                        L10n.App.Inspector.installMethod.localized,
                        selection: Binding(
                            get: { pendingInstallMethodRawValue ?? "" },
                            set: { pendingInstallMethodRawValue = $0 }
                        )
                    ) {
                        ForEach(pendingInstallMethodOptions) { option in
                            Text(installMethodLabel(option, includeTag: true))
                                .tag(option.method.rawValue)
                                .disabled(!installMethodOptionAllowed(option))
                        }
                    }
                    .pickerStyle(.inline)

                    if let unavailableMessage = selectedInstallMethodUnavailableMessage {
                        Text(unavailableMessage)
                            .font(.caption)
                            .foregroundColor(HelmTheme.stateAttention)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }

                if rustupInstallMethodSelected {
                    VStack(alignment: .leading, spacing: 8) {
                        Text(L10n.App.Inspector.installSource.localized)
                            .font(.caption.weight(.semibold))
                            .foregroundColor(.secondary)
                        Picker(
                            L10n.App.Inspector.installSource.localized,
                            selection: $pendingRustupInstallSource
                        ) {
                            Text(L10n.App.Inspector.InstallSource.officialDownload.localized)
                                .tag(ManagerRustupInstallSource.officialDownload)
                            Text(L10n.App.Inspector.InstallSource.existingBinaryPath.localized)
                                .tag(ManagerRustupInstallSource.existingBinaryPath)
                        }
                        .pickerStyle(.menu)

                        if pendingRustupInstallSource == .existingBinaryPath {
                            HStack(spacing: 8) {
                                TextField(
                                    L10n.App.Inspector.InstallSource.binaryPathPlaceholder.localized,
                                    text: $pendingRustupBinaryPath
                                )
                                .textFieldStyle(.roundedBorder)
                                .frame(minWidth: 260)

                                Button(L10n.App.Inspector.InstallSource.selectBinary.localized) {
                                    pickRustupBinaryPath()
                                }
                                .buttonStyle(HelmSecondaryButtonStyle())
                            }
                        }
                    }
                }

                if miseScriptInstallMethodSelected {
                    VStack(alignment: .leading, spacing: 8) {
                        Text(L10n.App.Inspector.installSource.localized)
                            .font(.caption.weight(.semibold))
                            .foregroundColor(.secondary)
                        Picker(
                            L10n.App.Inspector.installSource.localized,
                            selection: $pendingMiseInstallSource
                        ) {
                            Text(L10n.App.Inspector.InstallSource.officialDownload.localized)
                                .tag(ManagerMiseInstallSource.officialDownload)
                            Text("app.inspector.install_source.existing_mise_binary_path".localized)
                                .tag(ManagerMiseInstallSource.existingBinaryPath)
                        }
                        .pickerStyle(.menu)

                        if pendingMiseInstallSource == .existingBinaryPath {
                            HStack(spacing: 8) {
                                TextField(
                                    "app.inspector.install_source.binary_path_placeholder_mise".localized,
                                    text: $pendingMiseBinaryPath
                                )
                                .textFieldStyle(.roundedBorder)
                                .frame(minWidth: 260)

                                Button(L10n.App.Inspector.InstallSource.selectBinary.localized) {
                                    pickMiseBinaryPath()
                                }
                                .buttonStyle(HelmSecondaryButtonStyle())
                            }
                        }
                    }
                }

                if supportsPostInstallSetupAutomation {
                    Toggle(
                        "app.inspector.install.complete_post_install_setup_automatically".localized,
                        isOn: $pendingCompletePostInstallSetupAutomatically
                    )
                    .toggleStyle(.checkbox)
                    .font(.caption)
                }

                DisclosureGroup(
                    L10n.App.Settings.Section.advanced.localized,
                    isExpanded: $showAdvancedInstallOptions
                ) {
                    VStack(alignment: .leading, spacing: 10) {
                        timeoutSelectionRow(
                            label: L10n.App.Inspector.timeoutHard.localized,
                            options: hardTimeoutOptions,
                            selected: pendingHardTimeoutSeconds
                        ) { selection in
                            pendingHardTimeoutSeconds = selection
                        }

                        timeoutSelectionRow(
                            label: L10n.App.Inspector.timeoutIdle.localized,
                            options: idleTimeoutOptions,
                            selected: pendingIdleTimeoutSeconds
                        ) { selection in
                            pendingIdleTimeoutSeconds = selection
                        }
                    }
                    .padding(.top, 6)
                }

                HStack(spacing: 8) {
                    Spacer()
                    Button(L10n.Common.cancel.localized) {
                        showInstallOptionsSheet = false
                    }
                    .keyboardShortcut(.cancelAction)

                    Button(L10n.Common.install.localized) {
                        submitInstallWithSelectedMethod()
                    }
                    .buttonStyle(HelmPrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
                    .disabled(
                        installSubmissionInFlight
                            || pendingInstallMethodRawValue?.isEmpty != false
                            || !selectedPendingInstallMethodIsAllowed
                            || !rustupInstallSourceSelectionValid
                            || !miseInstallSourceSelectionValid
                    )
                }
            }
            .padding(20)
            .frame(minWidth: 420)
        }
        .sheet(item: $uninstallConfirmation) { confirmation in
            uninstallConfirmationSheet(confirmation)
        }
        .sheet(isPresented: $showPostInstallSetupSheet) {
            postInstallSetupSheet
        }
        .alert(item: $confirmAction) { action in
            switch action {
            case .update:
                return Alert(
                    title: Text(L10n.App.Managers.Alert.updateTitle.localized(with: ["manager": localizedManagerDisplayName(manager.id)])),
                    message: Text(L10n.App.Managers.Alert.updateMessage.localized),
                    primaryButton: .default(Text(L10n.Common.update.localized)) { core.updateManager(manager.id) },
                    secondaryButton: .cancel()
                )
            case let .enableRequiredManagerForInstance(parentManagerId, instanceId, followUp):
                return Alert(
                    title: Text(
                        L10n.App.Managers.Alert.enableRequiresParentTitle.localized(
                            with: ["manager": localizedManagerDisplayName(manager.id)]
                        )
                    ),
                    message: Text(
                        L10n.App.Managers.Alert.enableRequiresParentMessage.localized(
                            with: [
                                "manager": localizedManagerDisplayName(manager.id),
                                "parent": localizedManagerDisplayName(parentManagerId)
                            ]
                        )
                    ),
                    primaryButton: .default(Text(L10n.Common.continue.localized)) {
                        core.setManagerEnabled(parentManagerId, enabled: true) { success in
                            guard success else { return }
                            guard let instance = installInstances.first(where: { $0.instanceId == instanceId }) else {
                                return
                            }
                            performManagedInstanceSwitch(instance, followUp: followUp)
                        }
                    },
                    secondaryButton: .cancel(Text(L10n.Common.cancel.localized))
                )
            }
        }
        .onAppear {
            consumePendingInstallSheetRequestIfNeeded()
        }
        .onChange(of: context.managerInstallSheetRequestToken) { _ in
            consumePendingInstallSheetRequestIfNeeded()
        }
    }

    private func multiInstanceBanner<Actions: View>(
        icon: String,
        tint: Color,
        title: String,
        message: String,
        @ViewBuilder actions: () -> Actions
    ) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .top, spacing: 8) {
                Image(systemName: icon)
                    .foregroundColor(tint)
                    .padding(.top, 2)
                VStack(alignment: .leading, spacing: 4) {
                    Text(title)
                        .font(.callout.weight(.semibold))
                    Text(message)
                        .font(.caption)
                        .foregroundColor(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Spacer(minLength: 0)
            }
            HStack(spacing: 8) {
                actions()
                Spacer(minLength: 0)
            }
        }
        .padding(10)
        .background(
            RoundedRectangle(cornerRadius: 10)
                .fill(HelmTheme.surfaceElevated)
        )
    }

    @ViewBuilder
    private func packageStateIssueBanner(_ issue: ManagerPackageStateIssue) -> some View {
        let sourceManagerName = localizedManagerDisplayName(issue.sourceManagerId)
        let expectedPaths = metadataOnlyIssueExpectedPaths(for: issue)
        let detectedPaths = installInstances.map(\.displayPath)

        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .top, spacing: 8) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundColor(HelmTheme.stateAttention)
                    .padding(.top, 2)
                VStack(alignment: .leading, spacing: 4) {
                    Text(L10n.App.Inspector.PackageStateIssue.MetadataOnly.title.localized)
                        .font(.callout.weight(.semibold))
                    Text(
                        L10n.App.Inspector.PackageStateIssue.MetadataOnly.message.localized(with: [
                            "source_manager": sourceManagerName,
                            "package": issue.packageName
                        ])
                    )
                    .font(.caption)
                    .foregroundColor(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                    Text(L10n.App.Inspector.PackageStateIssue.MetadataOnly.impact.localized)
                        .font(.caption2)
                        .foregroundColor(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Spacer(minLength: 0)
            }
            HStack(spacing: 8) {
                Button(L10n.App.Inspector.PackageStateIssue.MetadataOnly.repairAction.localized) {
                    repairMetadataOnlyInstallIssue()
                }
                .buttonStyle(HelmSecondaryButtonStyle())
                .disabled(managerIsUninstalling || !metadataOnlyIssueCanRepairInstall)
                .helmPointer(enabled: !managerIsUninstalling && metadataOnlyIssueCanRepairInstall)

                Button(
                    L10n.App.Inspector.PackageStateIssue.MetadataOnly.removeStaleAction.localized
                ) {
                    removeMetadataOnlyInstallIssue()
                }
                .buttonStyle(HelmSecondaryButtonStyle())
                .disabled(!metadataOnlyIssueCanRemoveStaleEntry || managerIsUninstalling)
                .helmPointer(enabled: metadataOnlyIssueCanRemoveStaleEntry && !managerIsUninstalling)

                Spacer(minLength: 0)
            }

            DisclosureGroup(
                L10n.App.Inspector.PackageStateIssue.MetadataOnly.detailsToggle.localized,
                isExpanded: $showPackageStateIssueDetails
            ) {
                VStack(alignment: .leading, spacing: 8) {
                    HStack(alignment: .top, spacing: 8) {
                        Text(
                            "\(L10n.App.Inspector.PackageStateIssue.MetadataOnly.detailsSource.localized):"
                        )
                        .font(.caption.weight(.semibold))
                        .foregroundColor(.secondary)
                        Text(sourceManagerName)
                            .font(.caption)
                            .foregroundColor(.secondary)
                    }

                    HStack(alignment: .top, spacing: 8) {
                        Text(
                            "\(L10n.App.Inspector.PackageStateIssue.MetadataOnly.detailsPackage.localized):"
                        )
                        .font(.caption.weight(.semibold))
                        .foregroundColor(.secondary)
                        Text(issue.packageName)
                            .font(.system(.caption, design: .monospaced))
                            .foregroundColor(.secondary)
                    }

                    VStack(alignment: .leading, spacing: 4) {
                        Text(L10n.App.Inspector.PackageStateIssue.MetadataOnly.detailsExpectedPaths.localized)
                            .font(.caption.weight(.semibold))
                            .foregroundColor(.secondary)
                        if expectedPaths.isEmpty {
                            Text("-")
                                .font(.system(.caption, design: .monospaced))
                                .foregroundColor(.secondary)
                        } else {
                            ForEach(expectedPaths, id: \.self) { path in
                                Text(path)
                                    .font(.system(.caption, design: .monospaced))
                                    .foregroundColor(.secondary)
                            }
                        }
                    }

                    VStack(alignment: .leading, spacing: 4) {
                        Text(
                            "\(L10n.App.Inspector.PackageStateIssue.MetadataOnly.detailsDetectedInstances.localized): \(detectedPaths.count)"
                        )
                        .font(.caption.weight(.semibold))
                        .foregroundColor(.secondary)
                        if detectedPaths.isEmpty {
                            Text("-")
                                .font(.system(.caption, design: .monospaced))
                                .foregroundColor(.secondary)
                        } else {
                            ForEach(detectedPaths, id: \.self) { path in
                                Text(path)
                                    .font(.system(.caption, design: .monospaced))
                                    .foregroundColor(.secondary)
                            }
                        }
                    }
                }
                .padding(.top, 6)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(10)
        .background(
            RoundedRectangle(cornerRadius: 10)
                .fill(HelmTheme.surfaceElevated)
        )
    }

    @ViewBuilder
    private func postInstallSetupBanner(_ issue: ManagerPackageStateIssue) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .top, spacing: 8) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundColor(HelmTheme.stateAttention)
                    .padding(.top, 2)
                VStack(alignment: .leading, spacing: 4) {
                    Text("app.inspector.package_state_issue.setup_required.title".localized)
                        .font(.callout.weight(.semibold))
                    Text(
                        "app.inspector.package_state_issue.setup_required.message".localized(with: [
                            "manager": localizedManagerDisplayName(manager.id)
                        ])
                    )
                    .font(.caption)
                    .foregroundColor(.secondary)
                    .fixedSize(horizontal: false, vertical: true)

                    if let summary = issue.summary?.trimmingCharacters(in: .whitespacesAndNewlines),
                       !summary.isEmpty {
                        Text(summary)
                            .font(.caption2)
                            .foregroundColor(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                Spacer(minLength: 0)
            }

            VStack(alignment: .leading, spacing: 8) {
                Button(
                    "app.inspector.package_state_issue.setup_required.finish_action".localized(with: [
                        "manager": localizedManagerDisplayName(manager.id)
                    ])
                ) {
                    postInstallSetupVerificationMessage = nil
                    showPostInstallSetupSheet = true
                }
                .buttonStyle(HelmSecondaryButtonStyle())
                .disabled(managerIsUninstalling)
                .helmPointer(enabled: !managerIsUninstalling)

                if postInstallSetupAutomationAvailable {
                    Button("app.inspector.package_state_issue.setup_required.auto_action".localized) {
                        applyRecommendedPostInstallSetup()
                    }
                    .buttonStyle(HelmSecondaryButtonStyle())
                    .disabled(managerIsUninstalling)
                    .helmPointer(enabled: !managerIsUninstalling)
                }
            }
        }
        .padding(10)
        .background(
            RoundedRectangle(cornerRadius: 10)
                .fill(HelmTheme.surfaceElevated)
        )
    }

    @ViewBuilder
    private var postInstallSetupSheet: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(
                "app.inspector.package_state_issue.setup_required.modal_title".localized(with: [
                    "manager": localizedManagerDisplayName(manager.id)
                ])
            )
            .font(.title3.weight(.semibold))

            Text(
                "app.inspector.package_state_issue.setup_required.modal_message".localized(with: [
                    "manager": localizedManagerDisplayName(manager.id)
                ])
            )
            .font(.callout)
            .foregroundColor(.secondary)

            VStack(alignment: .leading, spacing: 6) {
                Text("app.inspector.package_state_issue.setup_required.steps_title".localized)
                    .font(.caption.weight(.semibold))
                    .foregroundColor(.secondary)
                ForEach(Array(postInstallSetupInstructions.enumerated()), id: \.offset) { index, instruction in
                    HStack(alignment: .top, spacing: 6) {
                        Text("\(index + 1).")
                            .font(.caption.monospacedDigit())
                            .foregroundColor(.secondary)
                        VStack(alignment: .leading, spacing: 6) {
                            Text(instruction.text)
                                .font(.caption)
                                .foregroundColor(.primary)
                                .fixedSize(horizontal: false, vertical: true)
                            ForEach(Array(instruction.commands.enumerated()), id: \.offset) { _, command in
                                HStack(alignment: .top, spacing: 8) {
                                    SelectableMonospacedTextArea(text: command)
                                        .frame(minHeight: 44, maxHeight: 60)
                                        .frame(maxWidth: .infinity, alignment: .leading)
                                        .background(
                                            RoundedRectangle(cornerRadius: 8)
                                                .fill(HelmTheme.surfaceElevated)
                                        )

                                    Button {
                                        copyTextToClipboard(command)
                                    } label: {
                                        Image(systemName: "doc.on.doc")
                                    }
                                    .buttonStyle(HelmIconButtonStyle())
                                    .help(L10n.App.Inspector.copyAll.localized)
                                    .accessibilityLabel(L10n.App.Inspector.copyAll.localized)
                                }
                            }
                        }
                    }
                }
            }

            if let message = postInstallSetupVerificationMessage,
               !message.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                Text(message)
                    .font(.caption)
                    .foregroundColor(HelmTheme.stateAttention)
                    .fixedSize(horizontal: false, vertical: true)
            }

            HStack(spacing: 8) {
                if postInstallSetupAutomationAvailable {
                    Button("app.inspector.package_state_issue.setup_required.auto_action".localized) {
                        postInstallSetupVerificationMessage = nil
                        showPostInstallSetupSheet = false
                        applyRecommendedPostInstallSetup()
                    }
                    .buttonStyle(HelmSecondaryButtonStyle())
                    .disabled(managerIsUninstalling)
                    .helmPointer(enabled: !managerIsUninstalling)
                }

                Spacer()

                Button(L10n.Common.cancel.localized) {
                    postInstallSetupVerificationMessage = nil
                    showPostInstallSetupSheet = false
                }
                .keyboardShortcut(.cancelAction)

                Button {
                    verifyPostInstallSetup()
                } label: {
                    HStack(spacing: 6) {
                        if verifyingPostInstallSetup {
                            ProgressView()
                                .controlSize(.small)
                        }
                        Text("app.inspector.package_state_issue.setup_required.verify_action".localized)
                    }
                }
                .buttonStyle(HelmPrimaryButtonStyle())
                .keyboardShortcut(.defaultAction)
                .disabled(verifyingPostInstallSetup || managerIsUninstalling)
                .helmPointer(enabled: !verifyingPostInstallSetup && !managerIsUninstalling)
            }
        }
        .padding(20)
        .frame(minWidth: 540)
    }

    private func applyRecommendedPostInstallSetup() {
        guard let issue = postInstallSetupIssue else { return }
        core.applyManagerPackageStateIssueRepair(
            managerId: manager.id,
            sourceManagerId: issue.sourceManagerId,
            packageName: issue.packageName,
            issueCode: issue.issueCode,
            optionId: "apply_post_install_setup_defaults"
        )
    }

    private var shellNameForSetupInstructions: String {
        let shellPath = ProcessInfo.processInfo.environment["SHELL"] ?? "/bin/zsh"
        let normalized = URL(fileURLWithPath: shellPath).lastPathComponent.lowercased()
        switch normalized {
        case "bash":
            return "bash"
        case "fish":
            return "fish"
        default:
            return "zsh"
        }
    }

    private var shellRcPathForSetupInstructions: String {
        switch shellNameForSetupInstructions {
        case "bash":
            return "~/.bashrc"
        case "fish":
            return "~/.config/fish/config.fish"
        default:
            return "~/.zshrc"
        }
    }

    private struct PostInstallSetupInstruction {
        let text: String
        let commands: [String]
    }

    private var postInstallSetupInstructions: [PostInstallSetupInstruction] {
        let shell = shellNameForSetupInstructions
        let rcFile = shellRcPathForSetupInstructions
        switch manager.id {
        case "rustup":
            return [
                PostInstallSetupInstruction(
                    text: "Open your shell startup file (\(rcFile)).",
                    commands: []
                ),
                PostInstallSetupInstruction(
                    text: "Add Cargo environment initialization so rustup-managed tools are on PATH.",
                    commands: ["echo 'source \"$HOME/.cargo/env\"' >> \(rcFile)"]
                ),
                PostInstallSetupInstruction(
                    text: "Start a new shell, then select Verify Setup. Or apply setup immediately in your current shell.",
                    commands: ["source \"$HOME/.cargo/env\""]
                )
            ]
        case "mise":
            return [
                PostInstallSetupInstruction(
                    text: "Open your shell startup file (\(rcFile)).",
                    commands: []
                ),
                PostInstallSetupInstruction(
                    text: "Add mise activation for \(shell).",
                    commands: ["echo 'eval \"$(mise activate \(shell))\"' >> \(rcFile)"]
                ),
                PostInstallSetupInstruction(
                    text: "Start a new shell, then select Verify Setup. Or apply setup immediately in your current shell.",
                    commands: ["eval \"$(mise activate \(shell))\""]
                )
            ]
        case "asdf":
            return [
                PostInstallSetupInstruction(
                    text: "Open your shell startup file (\(rcFile)).",
                    commands: []
                ),
                PostInstallSetupInstruction(
                    text: "Add asdf shims to PATH.",
                    commands: ["echo 'export PATH=\"${ASDF_DATA_DIR:-$HOME/.asdf}/shims:$PATH\"' >> \(rcFile)"]
                ),
                PostInstallSetupInstruction(
                    text: "Start a new shell, then select Verify Setup. Or apply setup immediately in your current shell.",
                    commands: ["export PATH=\"${ASDF_DATA_DIR:-$HOME/.asdf}/shims:$PATH\""]
                )
            ]
        default:
            return [
                PostInstallSetupInstruction(
                    text: "Complete the manager's documented post-install setup.",
                    commands: []
                ),
                PostInstallSetupInstruction(
                    text: "Start a new shell, then select Verify Setup.",
                    commands: []
                )
            ]
        }
    }

    private func verifyPostInstallSetup() {
        postInstallSetupVerificationMessage = nil
        verifyingPostInstallSetup = true
        core.triggerDetection(for: manager.id) { success in
            if !success {
                verifyingPostInstallSetup = false
                postInstallSetupVerificationMessage = L10n.Common.error.localized
                return
            }
            waitForPostInstallSetupVerificationResult(attemptsRemaining: 12)
        }
    }

    private func waitForPostInstallSetupVerificationResult(attemptsRemaining: Int) {
        core.fetchManagerStatus()
        core.fetchTasks()
        core.fetchPackages()
        core.fetchOutdatedPackages()

        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
            let diagnostics = core.managerDetectionDiagnostics(for: manager.id)
            if diagnostics.reason == .inProgress && attemptsRemaining > 0 {
                waitForPostInstallSetupVerificationResult(
                    attemptsRemaining: attemptsRemaining - 1
                )
                return
            }

            verifyingPostInstallSetup = false
            if let issue = postInstallSetupIssue {
                let summary = issue.summary?.trimmingCharacters(in: .whitespacesAndNewlines)
                if let summary, !summary.isEmpty {
                    postInstallSetupVerificationMessage = summary
                } else {
                    postInstallSetupVerificationMessage = localizedDetectionReason(diagnostics.reason)
                }
            } else {
                showPostInstallSetupSheet = false
            }
        }
    }

    private func repairMetadataOnlyInstallIssue() {
        guard !managerIsUninstalling,
              let issue = metadataOnlyPackageStateIssue else { return }
        core.applyManagerPackageStateIssueRepair(
            managerId: manager.id,
            sourceManagerId: issue.sourceManagerId,
            packageName: issue.packageName,
            issueCode: issue.issueCode,
            optionId: "reinstall_manager_via_homebrew"
        )
    }

    private func removeMetadataOnlyInstallIssue() {
        guard !managerIsUninstalling,
              let issue = metadataOnlyPackageStateIssue,
              metadataOnlyIssueCanRemoveStaleEntry else { return }
        core.applyManagerPackageStateIssueRepair(
            managerId: manager.id,
            sourceManagerId: issue.sourceManagerId,
            packageName: issue.packageName,
            issueCode: issue.issueCode,
            optionId: "remove_stale_package_entry"
        )
    }

    private func managerActionButton(
        symbol: String,
        tooltip: String,
        enabled: Bool,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
        }
        .buttonStyle(HelmIconButtonStyle())
        .help(tooltip)
        .accessibilityLabel(tooltip)
        .disabled(!enabled)
        .helmPointer(enabled: enabled)
    }

    private func consumePendingInstallSheetRequestIfNeeded() {
        guard context.managerInstallSheetRequestManagerId == manager.id else { return }
        context.managerInstallSheetRequestManagerId = nil
        guard managerCanInstall && !detected else { return }
        guard !managerIsUninstalling else { return }
        guard !installSubmissionInFlight else { return }
        prepareInstallMethodSelection()
    }

    private func performWithManagedInstance(
        _ instance: ManagerInstallInstanceStatus,
        followUp: ManagedInstanceFollowUpAction
    ) {
        guard !instance.isActive else {
            executeManagedInstanceFollowUp(followUp)
            return
        }

        if let parentManagerId = requiredDependencyManagerId(for: instance),
           let parentStatus = core.managersState.managerStatusesById[parentManagerId],
           !parentStatus.enabled
        {
            confirmAction = .enableRequiredManagerForInstance(
                parentManagerId: parentManagerId,
                instanceId: instance.instanceId,
                followUp: followUp
            )
            return
        }

        performManagedInstanceSwitch(instance, followUp: followUp)
    }

    private func requiredDependencyManagerId(for instance: ManagerInstallInstanceStatus) -> String? {
        ManagerDependencyResolver.dependencyManagerId(for: manager.id, provenance: instance.provenance)
    }

    private func performManagedInstanceSwitch(
        _ instance: ManagerInstallInstanceStatus,
        followUp: ManagedInstanceFollowUpAction
    ) {
        activeInstanceUpdateInFlightId = instance.instanceId
        core.setManagerActiveInstallInstance(manager.id, instanceId: instance.instanceId) { success in
            activeInstanceUpdateInFlightId = nil
            guard success else { return }
            executeManagedInstanceFollowUp(followUp)
        }
    }

    private func executeManagedInstanceFollowUp(_ followUp: ManagedInstanceFollowUpAction) {
        switch followUp {
        case .none:
            return
        case .update:
            confirmAction = .update
        case let .uninstall(allowUnknownProvenance):
            requestManagerUninstallConfirmation(allowUnknownProvenance: allowUnknownProvenance)
        }
    }

    private func prepareInstallMethodSelection() {
        let supportedOptions = sortedHelmSupportedInstallMethodOptions
        guard !supportedOptions.isEmpty else {
            core.installManager(manager.id)
            return
        }

        pendingInstallMethodOptions = supportedOptions
        pendingHardTimeoutSeconds = status?.timeoutHardSeconds
        pendingIdleTimeoutSeconds = status?.timeoutIdleSeconds
        showAdvancedInstallOptions = false
        pendingRustupInstallSource = .officialDownload
        pendingRustupBinaryPath = ""
        pendingMiseInstallSource = .officialDownload
        pendingMiseBinaryPath = ""
        pendingCompletePostInstallSetupAutomatically = false

        let allowedOptions = supportedOptions.filter(installMethodOptionAllowed)
        let selectedMethodRaw = selectedInstallMethodOption.method.rawValue
        if allowedOptions.contains(where: { $0.method.rawValue == selectedMethodRaw }) {
            pendingInstallMethodRawValue = selectedMethodRaw
        } else {
            pendingInstallMethodRawValue =
                allowedOptions.first(where: \.isRecommended)?.method.rawValue
                ?? allowedOptions.first(where: \.isPreferred)?.method.rawValue
                ?? allowedOptions.first?.method.rawValue
        }
        showInstallOptionsSheet = true
    }

    private func submitInstallWithSelectedMethod() {
        guard let installMethod = pendingInstallMethodRawValue, !installMethod.isEmpty else {
            return
        }
        guard let option = pendingInstallMethodOptions.first(where: { $0.method.rawValue == installMethod }),
              installMethodOptionAllowed(option) else {
            return
        }
        guard rustupInstallSourceSelectionValid else {
            return
        }

        installSubmissionInFlight = true
        core.setManagerInstallMethod(manager.id, installMethod: installMethod) { success in
            guard success else {
                installSubmissionInFlight = false
                return
            }
            core.setManagerTimeoutProfile(
                manager.id,
                hardTimeoutSeconds: pendingHardTimeoutSeconds,
                idleTimeoutSeconds: pendingIdleTimeoutSeconds
            ) { timeoutApplied in
                installSubmissionInFlight = false
                guard timeoutApplied else { return }
                showInstallOptionsSheet = false
                core.installManager(
                    manager.id,
                    options: installActionOptions(for: installMethod)
                )
            }
        }
    }

    private func installActionOptions(for installMethod: String) -> ManagerInstallActionOptions? {
        let autoCompleteSetup = pendingCompletePostInstallSetupAutomatically
        guard manager.id == "rustup",
              installMethod == ManagerDistributionMethod.rustupInstaller.rawValue else {
            if manager.id == "mise",
               installMethod == ManagerDistributionMethod.scriptInstaller.rawValue {
                let binaryPath = pendingMiseBinaryPath.trimmingCharacters(in: .whitespacesAndNewlines)
                return ManagerInstallActionOptions(
                    rustupInstallSource: nil,
                    rustupBinaryPath: nil,
                    miseInstallSource: pendingMiseInstallSource,
                    miseBinaryPath: binaryPath.isEmpty ? nil : binaryPath,
                    completePostInstallSetupAutomatically: autoCompleteSetup
                )
            }
            if supportsPostInstallSetupAutomation && autoCompleteSetup {
                return ManagerInstallActionOptions(
                    rustupInstallSource: nil,
                    rustupBinaryPath: nil,
                    miseInstallSource: nil,
                    miseBinaryPath: nil,
                    completePostInstallSetupAutomatically: true
                )
            }
            return nil
        }
        let binaryPath = pendingRustupBinaryPath.trimmingCharacters(in: .whitespacesAndNewlines)
        return ManagerInstallActionOptions(
            rustupInstallSource: pendingRustupInstallSource,
            rustupBinaryPath: binaryPath.isEmpty ? nil : binaryPath,
            miseInstallSource: nil,
            miseBinaryPath: nil,
            completePostInstallSetupAutomatically: autoCompleteSetup
        )
    }

    private func pickRustupBinaryPath() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        panel.resolvesAliases = true
        panel.prompt = L10n.Common.ok.localized
        if panel.runModal() == .OK, let url = panel.url {
            pendingRustupBinaryPath = url.path
        }
    }

    private func pickMiseBinaryPath() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        panel.resolvesAliases = true
        panel.prompt = L10n.Common.ok.localized
        if panel.runModal() == .OK, let url = panel.url {
            pendingMiseBinaryPath = url.path
        }
    }

    @ViewBuilder
    private func timeoutSelectionRow(
        label: String,
        options: [Int?],
        selected: Int?,
        onSelect: @escaping (Int?) -> Void
    ) -> some View {
        HStack(spacing: 8) {
            Text(label)
                .font(.caption)
                .foregroundColor(.secondary)
            Spacer(minLength: 8)
            Menu {
                ForEach(options, id: \.self) { seconds in
                    Button {
                        onSelect(seconds)
                    } label: {
                        HStack(spacing: 8) {
                            Text(timeoutMenuLabel(seconds))
                            if seconds == selected {
                                Image(systemName: "checkmark")
                            }
                        }
                    }
                }
            } label: {
                HStack(spacing: 6) {
                    Text(timeoutMenuLabel(selected))
                        .font(.caption.monospacedDigit())
                    Image(systemName: "chevron.down")
                        .font(.caption2)
                        .foregroundColor(.secondary)
                }
            }
            .menuStyle(.borderlessButton)
        }
    }

    private func requestManagerUninstallConfirmation(allowUnknownProvenance: Bool) {
        pendingUninstallOptions = defaultUninstallOptions(
            allowUnknownProvenance: allowUnknownProvenance
        )
        showUninstallDetails = false
        fetchManagerUninstallPreview(allowUnknownProvenance: allowUnknownProvenance)
    }

    private func defaultUninstallOptions(allowUnknownProvenance: Bool) -> ManagerUninstallActionOptions {
        return ManagerUninstallActionOptions(
            allowUnknownProvenance: allowUnknownProvenance,
            homebrewCleanupMode: nil,
            miseCleanupMode: nil,
            miseConfigRemoval: nil,
            removeHelmManagedShellSetup: supportsShellSetupTeardownOption
                ? defaultShellSetupTeardownSelection
                : nil
        )
    }

    private func fetchManagerUninstallPreview(allowUnknownProvenance: Bool) {
        loadingManagerUninstallPreview = true
        let options = ManagerUninstallActionOptions(
            allowUnknownProvenance: allowUnknownProvenance,
            homebrewCleanupMode: pendingUninstallOptions.homebrewCleanupMode,
            miseCleanupMode: pendingUninstallOptions.miseCleanupMode,
            miseConfigRemoval: pendingUninstallOptions.miseConfigRemoval,
            removeHelmManagedShellSetup: pendingUninstallOptions.removeHelmManagedShellSetup
        )
        core.previewManagerUninstall(
            manager.id,
            options: options
        ) { preview in
            loadingManagerUninstallPreview = false
            uninstallConfirmation = UninstallConfirmationContext(
                preview: preview,
                allowUnknownProvenance: allowUnknownProvenance
            )
        }
    }

    private func refreshManagerUninstallPreviewForCurrentOptions() {
        guard let context = uninstallConfirmation else { return }
        guard uninstallConfigSelectionValid else {
            uninstallConfirmation = UninstallConfirmationContext(
                preview: nil,
                allowUnknownProvenance: context.allowUnknownProvenance
            )
            return
        }
        fetchManagerUninstallPreview(
            allowUnknownProvenance: context.allowUnknownProvenance
        )
    }

    private func uninstallImpactSummary(_ context: UninstallConfirmationContext) -> String {
        let filesCount = context.preview?.filesRemoved.count ?? 0
        let directoriesCount = context.preview?.directoriesRemoved.count ?? 0
        let effectsCount = uninstallSecondaryEffects(context).count
        return L10n.App.Managers.Uninstall.Details.impactCounts.localized(with: [
            "files": filesCount,
            "directories": directoriesCount,
            "effects": effectsCount
        ])
    }

    private func uninstallSecondaryEffects(_ context: UninstallConfirmationContext) -> [String] {
        guard let preview = context.preview else {
            return []
        }
        return preview.secondaryEffects
    }

    @ViewBuilder
    private func uninstallConfirmationSheet(_ context: UninstallConfirmationContext) -> some View {
        VStack(alignment: .leading, spacing: 16) {
            Text(
                L10n.App.Managers.Alert.uninstallTitle.localized(
                    with: ["manager": localizedManagerDisplayName(manager.id)]
                )
            )
            .font(.title3.weight(.semibold))

            Text(
                L10n.App.Managers.Alert.uninstallMessage.localized(
                    with: ["manager_short": manager.shortName]
                )
            )
            .font(.callout)
            .foregroundColor(.secondary)

            let effects = uninstallSecondaryEffects(context)
            if !effects.isEmpty {
                VStack(alignment: .leading, spacing: 8) {
                    ForEach(effects, id: \.self) { effect in
                        HStack(alignment: .top, spacing: 8) {
                            Circle()
                                .fill(HelmTheme.stateAttention)
                                .frame(width: 6, height: 6)
                                .padding(.top, 6)
                            Text(effect)
                                .font(.callout)
                                .foregroundColor(.secondary)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                }
            }

            let usesHomebrewCleanupScope = context.preview?.strategy == "homebrew_formula"
            if usesHomebrewCleanupScope {
                VStack(alignment: .leading, spacing: 10) {
                    Text("app.managers.uninstall.scope".localized)
                        .font(.caption.weight(.semibold))
                        .foregroundColor(.secondary)
                    Picker(
                        "app.managers.uninstall.scope".localized,
                        selection: Binding(
                            get: { homebrewUninstallCleanupModeSelection },
                            set: { mode in
                                pendingUninstallOptions = ManagerUninstallActionOptions(
                                    allowUnknownProvenance: pendingUninstallOptions.allowUnknownProvenance,
                                    homebrewCleanupMode: mode,
                                    miseCleanupMode: nil,
                                    miseConfigRemoval: nil,
                                    removeHelmManagedShellSetup: pendingUninstallOptions.removeHelmManagedShellSetup
                                )
                                refreshManagerUninstallPreviewForCurrentOptions()
                            }
                        )
                    ) {
                        Text("app.managers.uninstall.scope.manager_only".localized)
                            .tag(ManagerHomebrewUninstallCleanupMode.managerOnly)
                        Text("app.managers.uninstall.scope.full_cleanup".localized)
                            .tag(ManagerHomebrewUninstallCleanupMode.fullCleanup)
                    }
                    .pickerStyle(.segmented)
                }
            } else if manager.id == "mise" {
                VStack(alignment: .leading, spacing: 10) {
                    Text("app.managers.uninstall.scope".localized)
                        .font(.caption.weight(.semibold))
                        .foregroundColor(.secondary)
                    Picker(
                        "app.managers.uninstall.scope".localized,
                        selection: Binding(
                            get: { miseUninstallCleanupModeSelection },
                            set: { mode in
                                if mode == .managerOnly {
                                    pendingUninstallOptions = ManagerUninstallActionOptions(
                                        allowUnknownProvenance: pendingUninstallOptions.allowUnknownProvenance,
                                        homebrewCleanupMode: nil,
                                        miseCleanupMode: .managerOnly,
                                        miseConfigRemoval: nil,
                                        removeHelmManagedShellSetup: pendingUninstallOptions.removeHelmManagedShellSetup
                                    )
                                } else {
                                    pendingUninstallOptions = ManagerUninstallActionOptions(
                                        allowUnknownProvenance: pendingUninstallOptions.allowUnknownProvenance,
                                        homebrewCleanupMode: nil,
                                        miseCleanupMode: .fullCleanup,
                                        miseConfigRemoval: pendingUninstallOptions.miseConfigRemoval,
                                        removeHelmManagedShellSetup: pendingUninstallOptions.removeHelmManagedShellSetup
                                    )
                                }
                                refreshManagerUninstallPreviewForCurrentOptions()
                            }
                        )
                    ) {
                        Text("app.managers.uninstall.scope.manager_only".localized)
                            .tag(ManagerMiseUninstallCleanupMode.managerOnly)
                        Text("app.managers.uninstall.scope.full_cleanup".localized)
                            .tag(ManagerMiseUninstallCleanupMode.fullCleanup)
                    }
                    .pickerStyle(.segmented)

                    if miseRequiresConfigRemovalSelection {
                        VStack(alignment: .leading, spacing: 8) {
                            Text("app.managers.uninstall.config".localized)
                                .font(.caption.weight(.semibold))
                                .foregroundColor(.secondary)
                            Picker(
                                "app.managers.uninstall.config".localized,
                                selection: Binding<ManagerMiseUninstallConfigRemoval?>(
                                    get: { pendingUninstallOptions.miseConfigRemoval },
                                    set: { selection in
                                        pendingUninstallOptions = ManagerUninstallActionOptions(
                                            allowUnknownProvenance: pendingUninstallOptions.allowUnknownProvenance,
                                            homebrewCleanupMode: nil,
                                            miseCleanupMode: .fullCleanup,
                                            miseConfigRemoval: selection,
                                            removeHelmManagedShellSetup: pendingUninstallOptions.removeHelmManagedShellSetup
                                        )
                                        refreshManagerUninstallPreviewForCurrentOptions()
                                    }
                                )
                            ) {
                                Text("app.managers.uninstall.config.required".localized)
                                    .tag(Optional<ManagerMiseUninstallConfigRemoval>.none)
                                Text("app.managers.uninstall.config.keep".localized)
                                    .tag(Optional(ManagerMiseUninstallConfigRemoval.keepConfig))
                                Text("app.managers.uninstall.config.remove".localized)
                                    .tag(Optional(ManagerMiseUninstallConfigRemoval.removeConfig))
                            }
                            .pickerStyle(.menu)
                        }
                    }
                }
            }

            if supportsShellSetupTeardownOption {
                Toggle(
                    "app.managers.uninstall.remove_helm_managed_shell_setup".localized,
                    isOn: Binding(
                        get: { shellSetupTeardownSelection },
                        set: { enabled in
                            pendingUninstallOptions = ManagerUninstallActionOptions(
                                allowUnknownProvenance: pendingUninstallOptions.allowUnknownProvenance,
                                homebrewCleanupMode: pendingUninstallOptions.homebrewCleanupMode,
                                miseCleanupMode: pendingUninstallOptions.miseCleanupMode,
                                miseConfigRemoval: pendingUninstallOptions.miseConfigRemoval,
                                removeHelmManagedShellSetup: enabled
                            )
                            refreshManagerUninstallPreviewForCurrentOptions()
                        }
                    )
                )
                .toggleStyle(.checkbox)
                .font(.caption)
            }

            DisclosureGroup(
                L10n.App.Managers.Uninstall.Details.toggle.localized,
                isExpanded: $showUninstallDetails
            ) {
                VStack(alignment: .leading, spacing: 10) {
                    HStack(alignment: .top, spacing: 8) {
                        Text("\(L10n.App.Managers.Uninstall.Details.strategy.localized):")
                            .font(.caption.weight(.semibold))
                            .foregroundColor(.secondary)
                        Text(context.preview?.strategy ?? "legacy")
                            .font(.caption.monospacedDigit())
                            .foregroundColor(.secondary)
                    }

                    HStack(alignment: .top, spacing: 8) {
                        Text("\(L10n.App.Managers.Uninstall.Details.impacts.localized):")
                            .font(.caption.weight(.semibold))
                            .foregroundColor(.secondary)
                        Text(uninstallImpactSummary(context))
                            .font(.caption)
                            .foregroundColor(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                .padding(.top, 6)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            HStack(spacing: 8) {
                Spacer()
                if context.readOnlyBlocked {
                    Button(L10n.Common.ok.localized) {
                        uninstallConfirmation = nil
                    }
                    .buttonStyle(HelmPrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
                } else {
                    Button(L10n.Common.cancel.localized) {
                        uninstallConfirmation = nil
                    }
                    .keyboardShortcut(.cancelAction)

                    Button(L10n.Common.uninstall.localized) {
                        let effectiveOptions = ManagerUninstallActionOptions(
                            allowUnknownProvenance: context.resolvedAllowUnknownProvenance,
                            homebrewCleanupMode: pendingUninstallOptions.homebrewCleanupMode,
                            miseCleanupMode: pendingUninstallOptions.miseCleanupMode,
                            miseConfigRemoval: pendingUninstallOptions.miseConfigRemoval,
                            removeHelmManagedShellSetup: pendingUninstallOptions.removeHelmManagedShellSetup
                        )
                        uninstallConfirmation = nil
                        core.uninstallManager(manager.id, options: effectiveOptions)
                    }
                    .buttonStyle(HelmPrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
                    .disabled(loadingManagerUninstallPreview || !uninstallConfigSelectionValid)
                }
            }
        }
        .padding(20)
        .frame(minWidth: 460)
        .onAppear {
            showUninstallDetails = false
        }
    }

    private func localizedCategoryName(_ category: String) -> String {
        switch category {
        case "Toolchain": return L10n.App.Managers.Category.toolchain.localized
        case "System/OS": return L10n.App.Managers.Category.systemOs.localized
        case "Language": return L10n.App.Managers.Category.language.localized
        case "App Store": return L10n.App.Managers.Category.appStore.localized
        default: return category
        }
    }

    private var pendingInstallMethodDisplayName: String {
        guard let pending = pendingInstallMethodRawValue,
              let option = pendingInstallMethodOptions.first(where: {
                  $0.method.rawValue == pending
              }) else {
            return localizedInstallMethod(selectedInstallMethodOption.method)
        }
        return localizedInstallMethod(option.method)
    }

    private func localizedInstallMethod(_ method: ManagerDistributionMethod) -> String {
        switch method {
        case .homebrew: return L10n.App.Inspector.InstallMethod.homebrew.localized
        case .macports: return L10n.App.Inspector.InstallMethod.macports.localized
        case .appStore: return L10n.App.Inspector.InstallMethod.appStore.localized
        case .setapp: return L10n.App.Inspector.InstallMethod.setapp.localized
        case .officialInstaller: return L10n.App.Inspector.InstallMethod.officialInstaller.localized
        case .scriptInstaller: return L10n.App.Inspector.InstallMethod.scriptInstaller.localized
        case .corepack: return L10n.App.Inspector.InstallMethod.corepack.localized
        case .rustupInstaller: return L10n.App.Inspector.InstallMethod.rustupInstaller.localized
        case .xcodeSelect: return L10n.App.Inspector.InstallMethod.xcodeSelect.localized
        case .softwareUpdate: return L10n.App.Inspector.InstallMethod.softwareUpdate.localized
        case .systemProvided: return L10n.App.Inspector.InstallMethod.systemProvided.localized
        case .npm: return L10n.App.Inspector.InstallMethod.npm.localized
        case .pip: return L10n.App.Inspector.InstallMethod.pip.localized
        case .pipx: return L10n.App.Inspector.InstallMethod.pipx.localized
        case .gem: return L10n.App.Inspector.InstallMethod.gem.localized
        case .cargoInstall: return L10n.App.Inspector.InstallMethod.cargoInstall.localized
        case .asdf: return L10n.App.Inspector.InstallMethod.asdf.localized
        case .mise: return L10n.App.Inspector.InstallMethod.mise.localized
        case .notManageable: return L10n.App.Inspector.InstallMethod.notManageable.localized
        }
    }

    private func installMethodLabel(_ option: ManagerInstallMethodOption, includeTag: Bool) -> String {
        var value = localizedInstallMethod(option.method)
        guard includeTag else { return value }
        var tags: [String] = []
        if option.isRecommended {
            tags.append(L10n.App.Inspector.installMethodTagRecommended.localized)
        } else if option.isPreferred {
            tags.append(L10n.App.Inspector.installMethodTagPreferred.localized)
        }
        switch option.policyTag {
        case .managedRestricted:
            tags.append(L10n.App.Inspector.installMethodTagManagedRestricted.localized)
        case .blockedByPolicy:
            tags.append(L10n.App.Inspector.installMethodTagBlocked.localized)
        case .allowed:
            break
        }
        if !tags.isEmpty {
            value += " (\(tags.joined(separator: ", ")))"
        }
        return value
    }

    private func installMethodOptionAllowed(_ option: ManagerInstallMethodOption) -> Bool {
        option.isAllowed(in: installMethodPolicyContext)
            && installMethodDependencyAvailable(option)
    }

    private func installMethodDependencyAvailable(_ option: ManagerInstallMethodOption) -> Bool {
        guard let dependencyManagerId = ManagerDependencyResolver.dependencyManagerId(
            for: manager.id,
            installMethod: option.method
        ) else {
            return true
        }
        guard core.isManagerDetected(dependencyManagerId) else {
            return false
        }
        guard let dependencyStatus = core.managerStatuses[dependencyManagerId] else {
            return false
        }
        return dependencyStatus.enabled
    }

    private func formatConfidence(_ value: Double) -> String {
        String(format: "%.2f", value)
    }

    private func toggleInstallInstanceExpansion(_ instanceId: String) {
        if expandedInstallInstanceIds.contains(instanceId) {
            expandedInstallInstanceIds.remove(instanceId)
        } else {
            expandedInstallInstanceIds.insert(instanceId)
        }
    }

    private func timeoutMenuLabel(_ seconds: Int?) -> String {
        guard let seconds else {
            return L10n.App.Inspector.timeoutUseDefault.localized
        }
        return L10n.App.Inspector.timeoutSeconds.localized(with: ["seconds": seconds])
    }

    private func localizedDetectionReason(_ reason: ManagerDetectionDiagnosticReason) -> String {
        switch reason {
        case .detected: return L10n.App.Inspector.detectionReasonDetected.localized
        case .notDetected, .neverChecked:
            return "\(localizedManagerDisplayName(manager.id)): \(L10n.App.Inspector.detectionReasonNotDetected.localized)"
        case .inconsistent: return L10n.App.Inspector.detectionReasonInconsistent.localized
        case .inProgress: return L10n.App.Inspector.detectionReasonInProgress.localized
        case .failed: return L10n.App.Inspector.detectionReasonFailed.localized
        case .disabled: return L10n.App.Inspector.detectionReasonDisabled.localized
        case .notImplemented: return L10n.App.Inspector.detectionReasonNotImplemented.localized
        }
    }

    private func detectionDiagnosticsIconName(_ reason: ManagerDetectionDiagnosticReason) -> String {
        switch reason {
        case .detected:
            return "checkmark.circle.fill"
        case .inconsistent:
            return "exclamationmark.triangle.fill"
        case .inProgress:
            return "clock.badge.exclamationmark"
        case .notDetected, .neverChecked:
            return "questionmark.circle"
        case .failed, .disabled, .notImplemented:
            return "xmark.circle"
        }
    }

    private func detectionDiagnosticsIconColor(_ reason: ManagerDetectionDiagnosticReason) -> Color {
        switch reason {
        case .detected:
            return HelmTheme.stateHealthy
        case .inconsistent:
            return HelmTheme.stateAttention
        case .inProgress:
            return HelmTheme.stateAttention
        case .notDetected, .neverChecked:
            return HelmTheme.textSecondary
        case .failed, .disabled, .notImplemented:
            return HelmTheme.stateError
        }
    }

    private func localizedTaskStatus(_ status: String) -> String {
        switch status.lowercased() {
        case "queued": return L10n.Service.Task.Status.pending.localized
        case "running": return L10n.Service.Task.Status.running.localized
        case "completed": return L10n.Service.Task.Status.completed.localized
        case "failed": return L10n.Service.Task.Status.failed.localized
        case "cancelled": return L10n.Service.Task.Status.cancelled.localized
        default: return status
        }
    }
}

// MARK: - Helper

private struct InspectorField<Content: View>: View {
    let label: String
    let content: Content

    init(label: String, @ViewBuilder content: () -> Content) {
        self.label = label
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label)
                .font(.caption)
                .foregroundColor(.secondary)
            content
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
    }
}
