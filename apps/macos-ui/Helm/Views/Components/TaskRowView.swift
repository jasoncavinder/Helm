import SwiftUI

struct TaskRowView: View {
    let task: TaskItem
    var onCancel: (() -> Void)? = nil

    var body: some View {
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
        .padding(.vertical, 3)
        .padding(.horizontal, 8)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(task.description)
        .accessibilityValue(task.localizedStatus)
    }
}
