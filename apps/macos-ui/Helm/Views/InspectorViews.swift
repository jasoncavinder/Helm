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
                            context.selectedManagerId = manager.id
                            context.selectedPackageId = nil
                            context.selectedTaskId = nil
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

        let textView = NSTextView()
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

private func copyTextToClipboard(_ text: String) {
    let pasteboard = NSPasteboard.general
    pasteboard.clearContents()
    pasteboard.setString(text, forType: .string)
}

// MARK: - Package Inspector

private struct InspectorPackageDetailView: View {
    @ObservedObject private var core = HelmCore.shared
    @EnvironmentObject private var context: ControlCenterContext
    @State private var renderedPackageDescription: PackageDescriptionRenderer.RenderedDescription?
    @State private var loadingPackageUninstallPreview = false
    @State private var confirmPackageUninstall: ConfirmPackageUninstallAction?
    let package: PackageItem

    private enum ConfirmPackageUninstallAction: Identifiable {
        case uninstall(preview: PackageUninstallPreview)
        case uninstallFallback

        var id: String {
            switch self {
            case let .uninstall(preview):
                return "uninstall-\(preview.packageName)-\(preview.blastRadiusScore)"
            case .uninstallFallback:
                return "uninstall-fallback"
            }
        }
    }

    private var supportsKegPolicyOverride: Bool {
        package.managerId == "homebrew_formula" && package.status != .available
    }

    private var kegPolicySelection: KegPolicySelection {
        core.kegPolicySelection(for: package)
    }

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

            if supportsKegPolicyOverride {
                kegPolicyMenuField
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

            packageActionRow

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
        .alert(item: $confirmPackageUninstall) { action in
            switch action {
            case let .uninstall(preview):
                let message = packageUninstallAlertMessage(preview)
                if preview.managerAutomationLevel == "read_only" {
                    return Alert(
                        title: Text(
                            L10n.App.Packages.Alert.uninstallTitle.localized(
                                with: ["package": package.name]
                            )
                        ),
                        message: Text(message),
                        dismissButton: .default(Text(L10n.Common.ok.localized))
                    )
                }
                return Alert(
                    title: Text(
                        L10n.App.Packages.Alert.uninstallTitle.localized(
                            with: ["package": package.name]
                        )
                    ),
                    message: Text(message),
                    primaryButton: .destructive(Text(L10n.Common.uninstall.localized)) {
                        core.uninstallPackage(package)
                    },
                    secondaryButton: .cancel()
                )
            case .uninstallFallback:
                return Alert(
                    title: Text(
                        L10n.App.Packages.Alert.uninstallTitle.localized(
                            with: ["package": package.name]
                        )
                    ),
                    message: Text(
                        L10n.App.Packages.Alert.uninstallMessage.localized(
                            with: [
                                "package": package.name,
                                "manager": localizedManagerDisplayName(package.managerId),
                            ]
                        )
                    ),
                    primaryButton: .destructive(Text(L10n.Common.uninstall.localized)) {
                        core.uninstallPackage(package)
                    },
                    secondaryButton: .cancel()
                )
            }
        }
    }

    private func refreshRenderedPackageDescription() {
        renderedPackageDescription = core.renderedPackageDescription(for: package)
    }

    private var kegPolicyMenuField: some View {
        InspectorField(label: L10n.App.Packages.Label.homebrewKegPolicy.localized) {
            Menu {
                Button {
                    core.setKegPolicySelection(for: package, selection: .useGlobal)
                } label: {
                    HStack(spacing: 8) {
                        Text(L10n.App.Packages.KegPolicy.useGlobal.localized)
                        if kegPolicySelection == .useGlobal {
                            Image(systemName: "checkmark")
                        }
                    }
                }

                Button {
                    core.setKegPolicySelection(for: package, selection: .keep)
                } label: {
                    HStack(spacing: 8) {
                        Text(L10n.App.Packages.KegPolicy.keepOld.localized)
                        if kegPolicySelection == .keep {
                            Image(systemName: "checkmark")
                        }
                    }
                }

                Button {
                    core.setKegPolicySelection(for: package, selection: .cleanup)
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

    private var packageActionRow: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 6) {
                if core.canInstallPackage(package) {
                    packageActionButton(
                        symbol: "arrow.down.circle",
                        tooltip: L10n.App.Packages.Action.install.localized,
                        enabled: !core.installActionPackageIds.contains(package.id)
                    ) {
                        core.installPackage(package)
                    }
                }

                if core.canUninstallPackage(package) {
                    packageActionButton(
                        symbol: "trash",
                        tooltip: L10n.App.Packages.Action.uninstall.localized,
                        enabled: !core.uninstallActionPackageIds.contains(package.id)
                            && !loadingPackageUninstallPreview
                    ) {
                        requestPackageUninstallConfirmation()
                    }
                }

                if core.canUpgradeIndividually(package) {
                    packageActionButton(
                        symbol: "arrow.up.circle",
                        tooltip: L10n.Common.update.localized,
                        enabled: !core.upgradeActionPackageIds.contains(package.id)
                    ) {
                        core.upgradePackage(package)
                    }
                }

                if core.canPinPackage(package) {
                    packageActionButton(
                        symbol: package.pinned ? "pin.slash" : "pin",
                        tooltip: package.pinned
                            ? L10n.App.Packages.Action.unpin.localized
                            : L10n.App.Packages.Action.pin.localized,
                        enabled: !core.pinActionPackageIds.contains(package.id)
                    ) {
                        if package.pinned {
                            core.unpinPackage(package)
                        } else {
                            core.pinPackage(package)
                        }
                    }
                }

                packageActionButton(
                    symbol: "slider.horizontal.3",
                    tooltip: L10n.App.Inspector.viewManager.localized,
                    enabled: true
                ) {
                    context.selectedManagerId = package.managerId
                    context.selectedPackageId = nil
                    context.selectedTaskId = nil
                    context.selectedUpgradePlanStepId = nil
                    context.selectedSection = .managers
                }

                Spacer(minLength: 0)
            }

            VStack(alignment: .leading, spacing: 2) {
                Text(L10n.App.Inspector.packageId.localized)
                    .font(.caption)
                    .foregroundColor(.secondary)
                Text(package.id)
                    .font(.caption.monospacedDigit())
                    .foregroundColor(.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityElement(children: .combine)
            .accessibilityLabel(L10n.App.Inspector.packageId.localized)
            .accessibilityValue(package.id)
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

    private func requestPackageUninstallConfirmation() {
        loadingPackageUninstallPreview = true
        core.previewPackageUninstall(package) { preview in
            loadingPackageUninstallPreview = false
            if let preview {
                confirmPackageUninstall = .uninstall(preview: preview)
                return
            }
            confirmPackageUninstall = .uninstallFallback
        }
    }

    private func packageUninstallAlertMessage(_ preview: PackageUninstallPreview) -> String {
        var sections = [
            L10n.App.Packages.Alert.uninstallMessage.localized(
                with: [
                    "package": package.name,
                    "manager": localizedManagerDisplayName(package.managerId),
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
    let onViewPackages: () -> Void
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
    @State private var pendingRustupInstallSource: ManagerRustupInstallSource = .officialDownload
    @State private var pendingRustupBinaryPath = ""
    @State private var pendingMiseInstallSource: ManagerMiseInstallSource = .officialDownload
    @State private var pendingMiseBinaryPath = ""
    @State private var activeInstanceUpdateInFlightId: String?
    @State private var pendingUninstallOptions = ManagerUninstallActionOptions(
        allowUnknownProvenance: false,
        miseCleanupMode: nil,
        miseConfigRemoval: nil
    )

    private enum ConfirmAction: Identifiable {
        case update

        var id: String {
            switch self {
            case .update:
                return "update"
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
        switch manager.id {
        case "mas":
            return ["homebrew"]
        default:
            return Set(resolvedInstallMethodOptions.map(\.method.rawValue))
        }
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
        guard let pendingInstallMethodRawValue,
              let option = pendingInstallMethodOptions.first(where: {
                  $0.method.rawValue == pendingInstallMethodRawValue
              }) else {
            return false
        }
        return installMethodOptionAllowed(option)
    }

    private var hasAllowedInstallMethodOption: Bool {
        sortedHelmSupportedInstallMethodOptions.contains(where: installMethodOptionAllowed)
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
            if left.isActive != right.isActive {
                return left.isActive && !right.isActive
            }
            return left.displayPath.localizedStandardCompare(right.displayPath) == .orderedAscending
        }
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

            managerActionRow

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

                if installInstanceCount > 0 {
                    InspectorField(label: L10n.App.Inspector.installInstanceCount.localized) {
                        VStack(alignment: .leading, spacing: 8) {
                            Text(String(installInstanceCount))
                                .font(.callout.monospacedDigit())
                            ForEach(installInstances) { instance in
                                VStack(alignment: .leading, spacing: 4) {
                                    HStack(alignment: .top, spacing: 6) {
                                        if instance.isActive {
                                            Image(systemName: "checkmark.circle.fill")
                                                .foregroundColor(HelmTheme.stateHealthy)
                                                .padding(.top, 1)
                                        }
                                        Text(instance.displayPath)
                                            .font(.system(.caption, design: .monospaced))
                                            .lineLimit(2)
                                    }
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
                                    if !instance.isActive {
                                        let managingThisInstance = activeInstanceUpdateInFlightId == instance.instanceId
                                        Button(L10n.App.Inspector.MultiInstance.manageInstance.localized) {
                                            activeInstanceUpdateInFlightId = instance.instanceId
                                            core.setManagerActiveInstallInstance(manager.id, instanceId: instance.instanceId) { _ in
                                                activeInstanceUpdateInFlightId = nil
                                            }
                                        }
                                        .font(.caption)
                                        .buttonStyle(HelmSecondaryButtonStyle())
                                        .disabled(managerIsUninstalling || managingThisInstance)
                                        .helmPointer(enabled: !(managerIsUninstalling || managingThisInstance))
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
                        with: ["manager_short": manager.shortName]
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
        .alert(item: $confirmAction) { action in
            switch action {
            case .update:
                return Alert(
                    title: Text(L10n.App.Managers.Alert.updateTitle.localized(with: ["manager": localizedManagerDisplayName(manager.id)])),
                    message: Text(L10n.App.Managers.Alert.updateMessage.localized),
                    primaryButton: .default(Text(L10n.Common.update.localized)) { core.updateManager(manager.id) },
                    secondaryButton: .cancel()
                )
            }
        }
    }

    private var managerActionRow: some View {
        HStack(spacing: 6) {
            if detected {
                managerActionButton(
                    symbol: "shippingbox",
                    tooltip: L10n.App.Managers.Action.viewPackages.localized,
                    enabled: packageCount > 0 && enabled && !managerIsUninstalling
                ) {
                    onViewPackages()
                }
            }

            if manager.canUpdate && detected && enabled {
                managerActionButton(
                    symbol: "arrow.up.circle",
                    tooltip: L10n.Common.update.localized,
                    enabled: !managerIsUninstalling
                ) {
                    confirmAction = .update
                }
            }

            if manager.canUninstall && detected && enabled {
                managerActionButton(
                    symbol: "trash",
                    tooltip: L10n.Common.uninstall.localized,
                    enabled: !loadingManagerUninstallPreview && !managerIsUninstalling
                ) {
                    requestManagerUninstallConfirmation(allowUnknownProvenance: false)
                }
            }

            if manager.canInstall && !detected {
                managerActionButton(
                    symbol: "arrow.down.circle",
                    tooltip: L10n.Common.install.localized,
                    enabled: !installSubmissionInFlight
                        && hasAllowedInstallMethodOption
                        && !managerIsUninstalling
                ) {
                    prepareInstallMethodSelection()
                }
            }

            Spacer(minLength: 0)
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
        guard manager.id == "rustup",
              installMethod == ManagerDistributionMethod.rustupInstaller.rawValue else {
            if manager.id == "mise",
               installMethod == ManagerDistributionMethod.scriptInstaller.rawValue {
                let binaryPath = pendingMiseBinaryPath.trimmingCharacters(in: .whitespacesAndNewlines)
                return ManagerInstallActionOptions(
                    rustupInstallSource: nil,
                    rustupBinaryPath: nil,
                    miseInstallSource: pendingMiseInstallSource,
                    miseBinaryPath: binaryPath.isEmpty ? nil : binaryPath
                )
            }
            return nil
        }
        let binaryPath = pendingRustupBinaryPath.trimmingCharacters(in: .whitespacesAndNewlines)
        return ManagerInstallActionOptions(
            rustupInstallSource: pendingRustupInstallSource,
            rustupBinaryPath: binaryPath.isEmpty ? nil : binaryPath,
            miseInstallSource: nil,
            miseBinaryPath: nil
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
        if manager.id == "mise" {
            return ManagerUninstallActionOptions(
                allowUnknownProvenance: allowUnknownProvenance,
                miseCleanupMode: .managerOnly,
                miseConfigRemoval: nil
            )
        }
        return ManagerUninstallActionOptions(
            allowUnknownProvenance: allowUnknownProvenance,
            miseCleanupMode: nil,
            miseConfigRemoval: nil
        )
    }

    private func fetchManagerUninstallPreview(allowUnknownProvenance: Bool) {
        loadingManagerUninstallPreview = true
        let options = ManagerUninstallActionOptions(
            allowUnknownProvenance: allowUnknownProvenance,
            miseCleanupMode: pendingUninstallOptions.miseCleanupMode,
            miseConfigRemoval: pendingUninstallOptions.miseConfigRemoval
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

            if manager.id == "mise" {
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
                                        miseCleanupMode: .managerOnly,
                                        miseConfigRemoval: nil
                                    )
                                } else {
                                    pendingUninstallOptions = ManagerUninstallActionOptions(
                                        allowUnknownProvenance: pendingUninstallOptions.allowUnknownProvenance,
                                        miseCleanupMode: .fullCleanup,
                                        miseConfigRemoval: pendingUninstallOptions.miseConfigRemoval
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
                                            miseCleanupMode: .fullCleanup,
                                            miseConfigRemoval: selection
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
                            miseCleanupMode: pendingUninstallOptions.miseCleanupMode,
                            miseConfigRemoval: pendingUninstallOptions.miseConfigRemoval
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
    }

    private func formatConfidence(_ value: Double) -> String {
        String(format: "%.2f", value)
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
