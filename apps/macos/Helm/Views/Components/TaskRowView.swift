import SwiftUI

struct TaskRowView: View {
    let task: TaskRecord

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: task.state.symbolName)
                .foregroundStyle(task.state.color)

            VStack(alignment: .leading, spacing: 4) {
                Text(LocalizedStringKey(task.title))
                    .font(.body)
                HStack(spacing: 6) {
                    Text(task.managerDisplayName)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Text("dot.separator")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Text(LocalizedStringKey(task.state.localizationKey))
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(task.state.color)
                }
            }

            Spacer()
        }
        .padding(.vertical, 4)
    }
}

private extension TaskState {
    var symbolName: String {
        switch self {
        case .queued:
            return "clock"
        case .running:
            return "arrow.triangle.2.circlepath"
        case .succeeded:
            return "checkmark.circle"
        case .partialFailure:
            return "exclamationmark.triangle"
        case .failed:
            return "xmark.octagon"
        case .canceled:
            return "stop.circle"
        }
    }

    var color: Color {
        switch self {
        case .queued:
            return .secondary
        case .running:
            return .blue
        case .succeeded:
            return .green
        case .partialFailure:
            return .orange
        case .failed:
            return .red
        case .canceled:
            return .secondary
        }
    }
}
