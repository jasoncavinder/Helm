import SwiftUI

struct TaskRowView: View {
    let task: TaskItem
    var onCancel: (() -> Void)? = nil
    var canExpandDetails = false
    var isExpanded = false
    var isSelected = false
    var onToggleDetails: (() -> Void)? = nil
    var onSelect: (() -> Void)? = nil

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
                TaskRowLiveOutputView(task: task)
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
    private static let outputAnchorId = "task-output-bottom"
    private let refreshTimer = Timer.publish(every: 1.0, on: .main, in: .common).autoconnect()
    let task: TaskItem

    @State private var isLoadingOutput = false
    @State private var taskOutputLoadFailed = false
    @State private var taskOutputRecord: CoreTaskOutputRecord?

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(L10n.App.Inspector.taskCommand.localized)
                .font(.caption.weight(.semibold))
                .foregroundColor(.secondary)

            Text(commandText)
                .font(.system(size: 12, weight: .regular, design: .monospaced))
                .foregroundColor(commandIsUnavailable ? .secondary : .primary)

            Text(L10n.App.Inspector.taskOutputStdout.localized)
                .font(.caption.weight(.semibold))
                .foregroundColor(.secondary)

            ScrollViewReader { proxy in
                ScrollView {
                    Text(outputText)
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
                .onChange(of: outputText) { _ in
                    scrollToBottom(using: proxy)
                }
            }
        }
        .onAppear {
            loadTaskOutput(force: true)
        }
        .onReceive(refreshTimer) { _ in
            guard task.isRunning else { return }
            loadTaskOutput(force: true)
        }
    }

    private var hasNumericTaskId: Bool {
        Int64(task.id) != nil
    }

    private var commandText: String {
        if let command = taskOutputRecord?.command?.trimmingCharacters(in: .whitespacesAndNewlines),
           !command.isEmpty {
            return command
        }
        return core.diagnosticCommandHint(for: task) ?? L10n.App.Inspector.taskCommandUnavailable.localized
    }

    private var commandIsUnavailable: Bool {
        commandText == L10n.App.Inspector.taskCommandUnavailable.localized
    }

    private var outputText: String {
        if let output = taskOutputText(), !output.isEmpty {
            return output
        }
        if isLoadingOutput {
            return L10n.App.Inspector.taskOutputLoading.localized
        }
        if taskOutputLoadFailed {
            return L10n.App.Inspector.taskOutputLoadFailed.localized
        }
        return L10n.App.Inspector.taskOutputUnavailable.localized
    }

    private func taskOutputText() -> String? {
        var segments: [String] = []

        if let stdout = taskOutputRecord?.stdout?.trimmingCharacters(in: .whitespacesAndNewlines),
           !stdout.isEmpty {
            segments.append("\(L10n.App.Inspector.taskOutputStdout.localized):\n\(stdout)")
        }

        if let stderr = taskOutputRecord?.stderr?.trimmingCharacters(in: .whitespacesAndNewlines),
           !stderr.isEmpty {
            segments.append("\(L10n.App.Inspector.taskOutputStderr.localized):\n\(stderr)")
        }

        guard !segments.isEmpty else { return nil }
        return segments.joined(separator: "\n\n")
    }

    private func loadTaskOutput(force: Bool) {
        guard hasNumericTaskId else { return }
        if isLoadingOutput {
            return
        }
        if taskOutputRecord != nil && !force {
            return
        }

        isLoadingOutput = true
        taskOutputLoadFailed = false
        core.fetchTaskOutput(taskId: task.id) { output in
            DispatchQueue.main.async {
                self.isLoadingOutput = false
                if let output {
                    self.taskOutputRecord = output
                    self.taskOutputLoadFailed = false
                } else if self.taskOutputRecord == nil {
                    self.taskOutputLoadFailed = true
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
