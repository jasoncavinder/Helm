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
    private static let outputAnchorId = "task-output-bottom"
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

    @State private var isLoadingTaskLogs = false
    @State private var taskLogsLoadFailed = false
    @State private var taskLogRecords: [CoreTaskLogRecord] = []
    @State private var lastLogsRefreshAt: Date = .distantPast

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(L10n.App.Inspector.taskOutputLogs.localized)
                .font(.caption.weight(.semibold))
                .foregroundColor(.secondary)

            ScrollViewReader { proxy in
                ScrollView {
                    Text(logOutputText)
                        .font(.system(size: 12, weight: .regular, design: .monospaced))
                        .frame(maxWidth: .infinity, alignment: .leading)
                    Color.clear
                        .frame(height: 1)
                        .id(Self.outputAnchorId)
                }
                .frame(minHeight: 88, maxHeight: 140)
                .padding(8)
                .background(
                    RoundedRectangle(cornerRadius: 8, style: .continuous)
                        .fill(HelmTheme.surfacePanel)
                        .overlay(
                            RoundedRectangle(cornerRadius: 8, style: .continuous)
                                .strokeBorder(HelmTheme.borderSubtle.opacity(0.9), lineWidth: 0.8)
                        )
                )
                .onAppear {
                    scrollToBottom(using: proxy)
                }
                .onChange(of: logOutputText) { _ in
                    scrollToBottom(using: proxy)
                }
            }
        }
        .onAppear {
            loadTaskLogs(force: true)
        }
        .onChange(of: context.selectedSection) { _ in
            if shouldPollLogs {
                loadTaskLogs(force: true)
            }
        }
        .onReceive(refreshTimer) { _ in
            guard task.isRunning else { return }
            guard shouldPollLogs else { return }
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

    private var logOutputText: String {
        if let logsText = taskLogsText(), !logsText.isEmpty {
            return logsText
        }
        if isLoadingTaskLogs {
            return L10n.App.Inspector.taskOutputLoading.localized
        }
        if taskLogsLoadFailed {
            return L10n.App.Inspector.taskOutputLoadFailed.localized
        }
        return L10n.App.Inspector.taskLogsEmpty.localized
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

    private func scrollToBottom(using proxy: ScrollViewProxy) {
        DispatchQueue.main.async {
            proxy.scrollTo(Self.outputAnchorId, anchor: .bottom)
        }
    }
}
