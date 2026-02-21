import SwiftUI

struct TasksSectionView: View {
    @ObservedObject private var core = HelmCore.shared
    @EnvironmentObject private var context: ControlCenterContext
    @State private var expandedRunningTaskId: String?

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
                    .foregroundColor(.secondary)
                Spacer()
            } else {
                List(core.activeTasks) { task in
                    TaskRowView(
                        task: task,
                        onCancel: task.isRunning ? { core.cancelTask(task) } : nil,
                        canExpandDetails: task.isRunning,
                        isExpanded: expandedRunningTaskId == task.id,
                        onToggleDetails: {
                            if expandedRunningTaskId == task.id {
                                expandedRunningTaskId = nil
                            } else {
                                expandedRunningTaskId = task.id
                            }
                        }
                    )
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
        .onChange(of: core.activeTasks.map { "\($0.id):\($0.status)" }) { _ in
            collapseExpandedTaskIfNeeded()
        }
    }

    private func collapseExpandedTaskIfNeeded() {
        guard let expandedRunningTaskId else { return }
        let stillRunning = core.activeTasks.contains {
            $0.id == expandedRunningTaskId && $0.isRunning
        }
        if !stillRunning {
            self.expandedRunningTaskId = nil
        }
    }
}

// Backward compatibility wrapper for legacy references.
struct TaskListView: View {
    var maxTasks: Int = 10

    var body: some View {
        TasksSectionView()
    }
}
