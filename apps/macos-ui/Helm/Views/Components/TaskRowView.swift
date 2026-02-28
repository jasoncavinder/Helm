import SwiftUI

enum TaskOutputSurface {
    case controlCenter
    case popover
}

struct TaskRowView: View {
    let task: TaskItem
    var onCancel: (() -> Void)?
    var onDismiss: (() -> Void)?
    var canExpandDetails = false
    var isExpanded = false
    var isSelected = false
    var outputSurface: TaskOutputSurface = .controlCenter
    var onToggleDetails: (() -> Void)?
    var onSelect: (() -> Void)?

    var body: some View {
        VStack(alignment: .leading, spacing: isExpanded ? 8 : 0) {
            HStack(spacing: 8) {
                if task.isRunning {
                    ProgressView()
                        .scaleEffect(0.6)
                        .frame(width: 16, height: 16)
                        .accessibilityLabel(L10n.Service.Task.Status.running.localized)
                } else {
                    Image(systemName: task.statusIcon)
                        .foregroundColor(task.statusColor)
                        .frame(width: 16)
                        .accessibilityHidden(true)
                }

                Text(task.description)
                    .font(.subheadline)
                    .lineLimit(1)

                Spacer()

                Text(task.localizedStatus)
                    .font(.caption)
                    .foregroundColor(task.statusColor)

                if canExpandDetails, let onToggleDetails {
                    Button(action: onToggleDetails) {
                        Image(systemName: isExpanded ? "chevron.down" : "chevron.right")
                            .font(.caption.weight(.semibold))
                            .foregroundColor(.secondary)
                    }
                    .buttonStyle(.plain)
                    .helmPointer()
                    .accessibilityLabel(task.description)
                    .accessibilityValue(task.localizedStatus)
                }

                if task.isRunning, let onCancel {
                    Button(action: onCancel) {
                        Image(systemName: "xmark.circle")
                            .foregroundColor(.secondary)
                    }
                    .buttonStyle(.plain)
                    .help(L10n.App.Tasks.Action.cancel.localized)
                    .helmPointer()
                    .accessibilityLabel(L10n.App.Tasks.Action.cancel.localized)
                }

                if !task.isRunning,
                   task.status.lowercased() == "failed",
                   let onDismiss {
                    Button(action: onDismiss) {
                        Image(systemName: "xmark.circle.fill")
                            .foregroundColor(.secondary)
                    }
                    .buttonStyle(.plain)
                    .help(L10n.App.Tasks.Action.dismissFailed.localized)
                    .helmPointer()
                    .accessibilityLabel(L10n.App.Tasks.Action.dismissFailed.localized)
                }
            }
            .contentShape(Rectangle())
            .gesture(
                TapGesture().onEnded {
                    if canExpandDetails {
                        onToggleDetails?()
                    }
                    onSelect?()
                },
                including: .gesture
            )
            .helmPointer(enabled: canExpandDetails || onSelect != nil)

            if canExpandDetails && isExpanded {
                TaskRowLiveOutputView(task: task, outputSurface: outputSurface)
            }
        }
        .padding(.vertical, 3)
        .padding(.horizontal, 8)
        .background(
            RoundedRectangle(cornerRadius: 8, style: .continuous)
                .fill(isSelected ? HelmTheme.selectionFill : Color.clear)
                .overlay(
                    RoundedRectangle(cornerRadius: 8, style: .continuous)
                        .strokeBorder(isSelected ? HelmTheme.selectionStroke : Color.clear, lineWidth: 0.8)
                )
        )
    }
}

private struct TaskRowLiveOutputView: View {
    @ObservedObject private var core = HelmCore.shared
    @EnvironmentObject private var context: ControlCenterContext
    private static let taskLogFetchLimit = 80
    private static let timestampFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.dateStyle = .none
        formatter.timeStyle = .medium
        return formatter
    }()
    private let refreshTimer = Timer.publish(every: 1.0, on: .main, in: .common).autoconnect()
    let task: TaskItem
    let outputSurface: TaskOutputSurface

    @State private var isLoadingTaskOutput = false
    @State private var taskOutputLoadFailed = false
    @State private var taskOutputRecord: CoreTaskOutputRecord?
    @State private var lastOutputRefreshAt: Date = .distantPast
    @State private var isLoadingTaskLogs = false
    @State private var taskLogsLoadFailed = false
    @State private var taskLogRecords: [CoreTaskLogRecord] = []
    @State private var lastLogsRefreshAt: Date = .distantPast

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(L10n.App.Inspector.taskOutputLogs.localized)
                .font(.caption.weight(.semibold))
                .foregroundColor(.secondary)

            TaskSelectableMonospacedTextArea(text: liveOutputText)
                .frame(minHeight: 88, maxHeight: 140)
                .background(
                    RoundedRectangle(cornerRadius: 8, style: .continuous)
                        .fill(HelmTheme.surfacePanel)
                        .overlay(
                            RoundedRectangle(cornerRadius: 8, style: .continuous)
                                .strokeBorder(HelmTheme.borderSubtle.opacity(0.9), lineWidth: 0.8)
                        )
                )
        }
        .onAppear {
            loadTaskOutput(force: true)
            loadTaskLogs(force: true)
        }
        .onChange(of: context.selectedSection) { _ in
            if shouldPollLogs {
                loadTaskOutput(force: true)
                loadTaskLogs(force: true)
            }
        }
        .onReceive(refreshTimer) { _ in
            guard task.isRunning else { return }
            guard shouldPollLogs else { return }
            loadTaskOutput(force: false)
            loadTaskLogs(force: false)
        }
    }

    private var shouldPollLogs: Bool {
        switch outputSurface {
        case .popover:
            return true
        case .controlCenter:
            let section = context.selectedSection ?? .overview
            return section == .overview || section == .tasks
        }
    }

    private var hasNumericTaskId: Bool {
        Int64(task.id) != nil
    }

    private var sortedTaskLogRecords: [CoreTaskLogRecord] {
        taskLogRecords.sorted { lhs, rhs in
            if lhs.createdAtUnix == rhs.createdAtUnix {
                return lhs.id < rhs.id
            }
            return lhs.createdAtUnix < rhs.createdAtUnix
        }
    }

    private var liveOutputText: String {
        let sections = renderedOutputSections()
        if !sections.isEmpty {
            return sections.joined(separator: "\n\n")
        }
        if isLoadingTaskOutput || isLoadingTaskLogs {
            return L10n.App.Inspector.taskOutputLoading.localized
        }
        if taskOutputLoadFailed || taskLogsLoadFailed {
            return L10n.App.Inspector.taskOutputLoadFailed.localized
        }
        return L10n.App.Inspector.taskLogsEmpty.localized
    }

    private func renderedOutputSections() -> [String] {
        var sections: [String] = []
        if let logsText = taskLogsText(), !logsText.isEmpty {
            sections.append(formattedSection(
                title: L10n.App.Inspector.taskOutputLogs.localized.uppercased(),
                body: logsText
            ))
        }
        if let stderrText = normalizedOutputText(taskOutputRecord?.stderr) {
            sections.append(formattedSection(
                title: L10n.App.Inspector.taskOutputStderr.localized.uppercased(),
                body: stderrText
            ))
        }
        if let stdoutText = normalizedOutputText(taskOutputRecord?.stdout) {
            sections.append(formattedSection(
                title: L10n.App.Inspector.taskOutputStdout.localized.uppercased(),
                body: stdoutText
            ))
        }
        return sections
    }

    private func formattedSection(title: String, body: String) -> String {
        "[\(title)]\n\(body)"
    }

    private func normalizedOutputText(_ value: String?) -> String? {
        let trimmed = value?.trimmingCharacters(in: .whitespacesAndNewlines)
        if let trimmed, !trimmed.isEmpty {
            return trimmed
        }
        return nil
    }

    private func taskLogsText() -> String? {
        guard !sortedTaskLogRecords.isEmpty else { return nil }
        return sortedTaskLogRecords
            .map(formatTaskLogLine)
            .joined(separator: "\n")
    }

    private func formatTaskLogLine(_ entry: CoreTaskLogRecord) -> String {
        let timestamp = Self.timestampFormatter.string(from: entry.createdAtDate)
        let level = entry.level.uppercased()
        let status = entry.status?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        let statusSegment = status.isEmpty ? "" : " [\(status.uppercased())]"
        return "[\(timestamp)] [\(level)]\(statusSegment) \(entry.message)"
    }

    private func loadTaskOutput(force: Bool) {
        guard hasNumericTaskId else { return }
        let now = Date()
        if !force && now.timeIntervalSince(lastOutputRefreshAt) < 1.0 {
            return
        }
        if isLoadingTaskOutput {
            return
        }
        if !task.isRunning && !force && taskOutputRecord != nil {
            return
        }

        isLoadingTaskOutput = true
        lastOutputRefreshAt = now
        taskOutputLoadFailed = false
        core.fetchTaskOutput(taskId: task.id) { output in
            DispatchQueue.main.async {
                self.isLoadingTaskOutput = false
                if let output {
                    self.taskOutputRecord = output
                    self.taskOutputLoadFailed = false
                } else if self.taskOutputRecord == nil {
                    self.taskOutputLoadFailed = true
                }
            }
        }
    }

    private func loadTaskLogs(force: Bool) {
        guard hasNumericTaskId else { return }
        let now = Date()
        if !force && now.timeIntervalSince(lastLogsRefreshAt) < 1.0 {
            return
        }
        if isLoadingTaskLogs {
            return
        }
        if !task.isRunning && !force && !taskLogRecords.isEmpty {
            return
        }

        isLoadingTaskLogs = true
        lastLogsRefreshAt = now
        taskLogsLoadFailed = false
        core.fetchTaskLogs(taskId: task.id, limit: Self.taskLogFetchLimit) { logs in
            DispatchQueue.main.async {
                self.isLoadingTaskLogs = false
                if let logs {
                    self.taskLogRecords = logs
                    self.taskLogsLoadFailed = false
                } else if self.taskLogRecords.isEmpty {
                    self.taskLogsLoadFailed = true
                }
            }
        }
    }
}

private struct TaskSelectableMonospacedTextArea: NSViewRepresentable {
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
        DispatchQueue.main.async {
            textView.scrollToEndOfDocument(nil)
        }
        return scrollView
    }

    func updateNSView(_ nsView: NSScrollView, context: Context) {
        guard let textView = nsView.documentView as? NSTextView else { return }
        if textView.string != text {
            textView.string = text
            textView.scrollToEndOfDocument(nil)
        }
    }
}
