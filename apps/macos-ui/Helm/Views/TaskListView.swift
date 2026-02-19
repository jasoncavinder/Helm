import SwiftUI

struct TasksSectionView: View {
    @ObservedObject private var core = HelmCore.shared
    @EnvironmentObject private var context: ControlCenterContext

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text(ControlCenterSection.tasks.title)
                    .font(.title2.weight(.semibold))
                Spacer()
                if core.isRefreshing {
                    ProgressView()
                        .controlSize(.small)
                }
            }

            if core.activeTasks.isEmpty {
                Spacer()
                Text(L10n.App.TasksSection.empty.localized)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                Spacer()
            } else {
                List(core.activeTasks) { task in
                    TaskRowView(task: task, onCancel: task.isRunning ? { core.cancelTask(task) } : nil)
                        .contentShape(Rectangle())
                        .onTapGesture {
                            if let managerId = inferManagerId(from: task.description) {
                                context.selectedManagerId = managerId
                            }
                        }
                        .helmPointer()
                }
                .listStyle(.inset)
            }
        }
        .padding(20)
    }

    private func inferManagerId(from description: String) -> String? {
        let candidates = ManagerInfo.implemented
        return candidates.first {
            description.localizedCaseInsensitiveContains(localizedManagerDisplayName($0.id))
        }?.id
    }
}

// Backward compatibility wrapper for legacy references.
struct TaskListView: View {
    var maxTasks: Int = 10

    var body: some View {
        TasksSectionView()
    }
}
