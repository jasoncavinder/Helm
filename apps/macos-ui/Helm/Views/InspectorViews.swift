import SwiftUI

struct ControlCenterInspectorView: View {
    @ObservedObject private var core = HelmCore.shared
    @EnvironmentObject private var context: ControlCenterContext

    private var selectedTask: TaskItem? {
        guard let taskId = context.selectedTaskId else { return nil }
        return core.activeTasks.first { $0.id == taskId }
    }

    private var selectedPackage: PackageItem? {
        guard let packageId = context.selectedPackageId else { return nil }
        return core.allKnownPackages.first { $0.id == packageId }
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
                    InspectorPackageDetailView(package: package)
                } else if let manager = selectedManager {
                    InspectorManagerDetailView(
                        manager: manager,
                        status: core.managerStatuses[manager.id],
                        detectionDiagnostics: core.managerDetectionDiagnostics(for: manager.id),
                        health: core.health(forManagerId: manager.id),
                        packageCount: core.installedPackages.filter { $0.managerId == manager.id }.count,
                        outdatedCount: core.outdatedCount(forManagerId: manager.id),
                        onViewPackages: {
                            context.managerFilterId = manager.id
                            context.selectedUpgradePlanStepId = nil
                            context.selectedSection = .packages
                        }
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
                    suggestedCommand: diagnosticCommandHint()
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

    var body: some View {
        Group {
            if let normalizedText {
                ScrollView([.horizontal, .vertical]) {
                    Text(normalizedText)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .font(.caption.monospacedDigit())
                        .padding(8)
                }
                .background(
                    RoundedRectangle(cornerRadius: 8)
                        .fill(Color(NSColor.textBackgroundColor))
                )
            } else {
                Text(unavailableText)
                    .foregroundColor(.secondary)
            }
        }
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
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 8) {
                            ForEach(filteredLogs) { entry in
                                TaskLogRowView(entry: entry)
                            }
                        }
                    }
                }
            }

            if canLoadMore && !isLoading {
                Button(L10n.App.Inspector.taskLogsLoadMore.localized) {
                    onLoadMore()
                }
                .buttonStyle(HelmSecondaryButtonStyle())
                .font(.caption)
                .helmPointer()
            }
        }
    }
}

private struct TaskLogRowView: View {
    let entry: CoreTaskLogRecord

    private static let timestampFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.dateStyle = .none
        formatter.timeStyle = .medium
        return formatter
    }()

    private var levelColor: Color {
        switch entry.level.lowercased() {
        case "error":
            return .red
        case "warn":
            return .orange
        default:
            return .secondary
        }
    }

    private var statusText: String {
        switch entry.status?.lowercased() {
        case "queued":
            return L10n.Service.Task.Status.pending.localized
        case "running":
            return L10n.Service.Task.Status.running.localized
        case "completed":
            return L10n.Service.Task.Status.completed.localized
        case "cancelled":
            return L10n.Service.Task.Status.cancelled.localized
        case "failed":
            return L10n.Service.Task.Status.failed.localized
        default:
            return "-"
        }
    }

    private var levelText: String {
        switch entry.level.lowercased() {
        case "info":
            return L10n.App.Inspector.taskLogsLevelInfo.localized
        case "warn":
            return L10n.Common.warning.localized
        case "error":
            return L10n.Common.error.localized
        default:
            return entry.level.capitalized
        }
    }

    private var timestampText: String {
        Self.timestampFormatter.string(from: entry.createdAtDate)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
                Text(timestampText)
                    .font(.caption2.monospacedDigit())
                    .foregroundColor(.secondary)
                    .frame(width: 80, alignment: .leading)

                Text(levelText)
                    .font(.caption2.weight(.semibold))
                    .foregroundColor(levelColor)
                    .frame(width: 70, alignment: .leading)

                Text(statusText)
                    .font(.caption2)
                    .foregroundColor(.secondary)
                    .frame(width: 80, alignment: .leading)

                Text(localizedManagerDisplayName(entry.manager))
                    .font(.caption2)
                    .foregroundColor(.secondary)

                Spacer()
            }

            Text(entry.message)
                .font(.caption.monospacedDigit())
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(8)
        .background(
            RoundedRectangle(cornerRadius: 8)
                .fill(Color(NSColor.textBackgroundColor))
        )
    }
}

// MARK: - Package Inspector

private struct InspectorPackageDetailView: View {
    @ObservedObject private var core = HelmCore.shared
    @EnvironmentObject private var context: ControlCenterContext
    @State private var renderedPackageDescription: PackageDescriptionRenderer.RenderedDescription?
    let package: PackageItem

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(package.name)
                .font(.title3.weight(.semibold))

            InspectorField(label: L10n.App.Inspector.manager.localized) {
                Text(localizedManagerDisplayName(package.managerId))
                    .font(.callout)
            }

            // Status badge
            HStack(spacing: 6) {
                Image(systemName: package.status.iconName)
                    .foregroundColor(package.status.iconColor)
                Text(package.status.displayName)
                    .font(.callout.weight(.medium))
                    .foregroundColor(package.status.iconColor)
            }
            .accessibilityElement(children: .combine)
            .accessibilityLabel(L10n.App.Inspector.packageStatus.localized)
            .accessibilityValue(package.status.displayName)

            InspectorField(label: L10n.App.Inspector.installed.localized) {
                Text(package.version)
                    .font(.caption.monospacedDigit())
            }

            if let latest = package.latestVersion {
                InspectorField(label: L10n.App.Inspector.latest.localized) {
                    Text(latest)
                        .font(.caption.monospacedDigit())
                }
            }

            if package.pinned {
                HStack(spacing: 6) {
                    Image(systemName: "pin.fill")
                        .foregroundColor(.orange)
                        .font(.caption)
                    Text(L10n.App.Inspector.pinned.localized)
                        .font(.callout)
                }
                .accessibilityElement(children: .combine)
                .accessibilityLabel(L10n.App.Inspector.pinned.localized)
            }

            if package.restartRequired {
                HStack(spacing: 6) {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundColor(.orange)
                        .font(.caption)
                    Text(L10n.App.Inspector.restartRequired.localized)
                        .font(.callout)
                }
                .accessibilityElement(children: .combine)
                .accessibilityLabel(L10n.App.Inspector.restartRequired.localized)
            }

            InspectorField(label: L10n.App.Inspector.description.localized) {
                if let renderedPackageDescription {
                    switch renderedPackageDescription {
                    case .plain(let text):
                        Text(text)
                            .font(.caption)
                    case .rich(let attributed):
                        InspectorAttributedText(attributedText: attributed)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                } else if core.packageDescriptionLoadingIds.contains(package.id) {
                    Text(L10n.App.Inspector.descriptionLoading.localized)
                        .font(.caption)
                        .foregroundColor(.secondary)
                } else if core.packageDescriptionUnavailableIds.contains(package.id) {
                    Text(L10n.App.Inspector.descriptionUnavailable.localized)
                        .font(.caption)
                        .foregroundColor(.secondary)
                } else {
                    Text(L10n.App.Inspector.descriptionLoading.localized)
                        .font(.caption)
                        .foregroundColor(.secondary)
                }
            }

            HStack(spacing: 8) {
                if core.canInstallPackage(package) {
                    Button(L10n.App.Packages.Action.install.localized) {
                        core.installPackage(package)
                    }
                    .buttonStyle(HelmSecondaryButtonStyle())
                    .disabled(core.installActionPackageIds.contains(package.id))
                    .helmPointer(enabled: !core.installActionPackageIds.contains(package.id))
                }

                if core.canUninstallPackage(package) {
                    Button(L10n.App.Packages.Action.uninstall.localized) {
                        core.uninstallPackage(package)
                    }
                    .buttonStyle(HelmSecondaryButtonStyle())
                    .disabled(core.uninstallActionPackageIds.contains(package.id))
                    .helmPointer(enabled: !core.uninstallActionPackageIds.contains(package.id))
                }

                if core.canUpgradeIndividually(package) {
                    Button(L10n.Common.update.localized) {
                        core.upgradePackage(package)
                    }
                    .buttonStyle(HelmSecondaryButtonStyle())
                    .disabled(core.upgradeActionPackageIds.contains(package.id))
                    .helmPointer(enabled: !core.upgradeActionPackageIds.contains(package.id))
                }

                if core.canPinPackage(package) {
                    Button(package.pinned ? L10n.App.Packages.Action.unpin.localized : L10n.App.Packages.Action.pin.localized) {
                        if package.pinned {
                            core.unpinPackage(package)
                        } else {
                            core.pinPackage(package)
                        }
                    }
                    .buttonStyle(HelmSecondaryButtonStyle())
                    .disabled(core.pinActionPackageIds.contains(package.id))
                    .helmPointer(enabled: !core.pinActionPackageIds.contains(package.id))
                }

                Button(L10n.App.Inspector.viewManager.localized) {
                    context.selectedManagerId = package.managerId
                    context.selectedPackageId = nil
                    context.selectedTaskId = nil
                    context.selectedUpgradePlanStepId = nil
                    context.selectedSection = .managers
                }
                .buttonStyle(HelmSecondaryButtonStyle())
                .helmPointer()
            }
            .font(.caption)

            InspectorField(label: L10n.App.Inspector.packageId.localized) {
                Text(package.id)
                    .font(.caption.monospacedDigit())
                    .foregroundColor(.secondary)
                    .accessibilityLabel(L10n.App.Inspector.packageId.localized)
                    .accessibilityValue(package.id)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .onAppear {
            core.ensurePackageDescription(for: package)
            refreshRenderedPackageDescription()
        }
        .onChange(of: package.id) { _ in
            core.ensurePackageDescription(for: package)
            refreshRenderedPackageDescription()
        }
        .onChange(of: package.summary) { _ in
            core.ensurePackageDescription(for: package)
            refreshRenderedPackageDescription()
        }
    }

    private func refreshRenderedPackageDescription() {
        renderedPackageDescription = PackageDescriptionRenderer.render(package.summary)
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
    let manager: ManagerInfo
    let status: ManagerStatus?
    let detectionDiagnostics: ManagerDetectionDiagnostics
    let health: OperationalHealth
    let packageCount: Int
    let outdatedCount: Int
    let onViewPackages: () -> Void

    private var detected: Bool {
        status?.detected ?? false
    }

    private var activeExecutablePath: String? {
        status?.executablePath?.trimmingCharacters(in: .whitespacesAndNewlines)
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

    private var selectedInstallMethodOption: ManagerInstallMethodOption {
        manager.selectedInstallMethodOption(
            executablePath: activeExecutablePath,
            installedPackages: core.installedPackages
        )
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

            InspectorField(label: L10n.App.Inspector.category.localized) {
                Text(localizedCategoryName(manager.category))
                    .font(.callout)
            }

            Text(manager.authority.key.localized)
                .font(.callout)
                .foregroundColor(.secondary)

            Group {
                InspectorField(label: L10n.App.Inspector.detectionDiagnostics.localized) {
                    HStack(alignment: .top, spacing: 6) {
                        Image(systemName: detected ? "checkmark.circle.fill" : "xmark.circle")
                            .foregroundColor(detected ? HelmTheme.stateHealthy : HelmTheme.stateError)
                            .padding(.top, 1)
                        Text(localizedDetectionReason(detectionDiagnostics.reason))
                            .font(.callout)
                    }
                    .accessibilityElement(children: .combine)
                    .accessibilityValue(detected
                        ? L10n.App.Inspector.detected.localized
                        : L10n.App.Inspector.notDetected.localized)
                }

                if let lastStatus = detectionDiagnostics.latestTaskStatus {
                    InspectorField(label: L10n.App.Inspector.detectionLastTaskStatus.localized) {
                        Text(localizedTaskStatus(lastStatus))
                            .font(.callout)
                    }
                }

                if let lastTaskId = detectionDiagnostics.latestTaskId {
                    InspectorField(label: L10n.App.Inspector.detectionLastTaskId.localized) {
                        Text(String(lastTaskId))
                            .font(.caption.monospacedDigit())
                    }
                }

                if !executablePaths.isEmpty {
                    InspectorField(label: L10n.App.Inspector.executablePaths.localized) {
                        VStack(alignment: .leading, spacing: 2) {
                            ForEach(executablePaths, id: \.self) { path in
                                Text(path)
                                    .font(
                                        path == activeExecutablePath
                                            ? .caption.monospacedDigit().weight(.semibold)
                                            : .caption.monospacedDigit()
                                    )
                                    .lineLimit(2)
                            }
                        }
                        .accessibilityLabel(L10n.App.Inspector.executablePaths.localized)
                        .accessibilityValue(executablePaths.joined(separator: ", "))
                    }
                }

            }

            InspectorField(label: L10n.App.Inspector.installMethod.localized) {
                Menu {
                    ForEach(manager.installMethodOptions) { option in
                        Button {} label: {
                            HStack(spacing: 8) {
                                Text(installMethodLabel(option, includeTag: true))
                                if option.method == selectedInstallMethodOption.method {
                                    Image(systemName: "checkmark")
                                }
                            }
                        }
                        .disabled(true)
                    }
                } label: {
                    HStack(spacing: 6) {
                        Text(installMethodLabel(selectedInstallMethodOption, includeTag: true))
                            .font(.callout)
                        Image(systemName: "chevron.down")
                            .font(.caption2)
                            .foregroundColor(.secondary)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                .menuStyle(.borderlessButton)
            }

            InspectorField(label: L10n.App.Inspector.capabilities.localized) {
                VStack(alignment: .leading, spacing: 2) {
                    ForEach(manager.capabilities, id: \.self) { capabilityKey in
                        Text(capabilityKey.localized)
                            .font(.caption)
                    }
                }
            }

            if packageCount > 0 {
                Button(L10n.App.Managers.Action.viewPackages.localized) {
                    onViewPackages()
                }
                .font(.caption)
                .helmPointer()
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
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
        if option.isRecommended {
            value += " (\(L10n.App.Inspector.installMethodTagRecommended.localized))"
        } else if option.isPreferred {
            value += " (\(L10n.App.Inspector.installMethodTagPreferred.localized))"
        }
        return value
    }

    private func localizedDetectionReason(_ reason: ManagerDetectionDiagnosticReason) -> String {
        switch reason {
        case .detected: return L10n.App.Inspector.detectionReasonDetected.localized
        case .notDetected: return L10n.App.Inspector.detectionReasonNotDetected.localized
        case .inProgress: return L10n.App.Inspector.detectionReasonInProgress.localized
        case .failed: return L10n.App.Inspector.detectionReasonFailed.localized
        case .disabled: return L10n.App.Inspector.detectionReasonDisabled.localized
        case .notImplemented: return L10n.App.Inspector.detectionReasonNotImplemented.localized
        case .neverChecked: return L10n.App.Inspector.detectionReasonNeverChecked.localized
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
    @ViewBuilder let content: Content

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
