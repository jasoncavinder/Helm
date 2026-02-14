import SwiftUI

struct TaskRowView: View {
    let task: TaskItem

    var body: some View {
        HStack(spacing: 8) {
            if task.isRunning {
                ProgressView()
                    .scaleEffect(0.6)
                    .frame(width: 16, height: 16)
            } else {
                Image(systemName: task.statusIcon)
                    .foregroundColor(task.statusColor)
                    .frame(width: 16)
            }

            Text(task.description)
                .font(.subheadline)
                .lineLimit(1)

            Spacer()

            Text(task.localizedStatus)
                .font(.caption)
                .foregroundColor(task.statusColor)

            if task.isRunning {
                Button(action: {}) {
                    Image(systemName: "xmark.circle")
                        .foregroundColor(.secondary)
                }
                .buttonStyle(.plain)
                .disabled(true)
                .opacity(0.5)
                .help("Cancel not yet implemented")
            }
        }
        .padding(.vertical, 3)
        .padding(.horizontal, 8)
    }
}
