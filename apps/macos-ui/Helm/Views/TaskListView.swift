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
                            context.selectedTaskId = task.id
                            context.selectedPackageId = nil
                            context.selectedUpgradePlanStepId = nil
                            if let managerId = task.managerId {
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
}

// Backward compatibility wrapper for legacy references.
struct TaskListView: View {
    var maxTasks: Int = 10

    var body: some View {
        TasksSectionView()
    }
}
