import SwiftUI

struct TasksSectionView: View {
    @ObservedObject private var core = HelmCore.shared
    @EnvironmentObject private var context: ControlCenterContext
    @State private var expandedTaskId: String?

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
                        canExpandDetails: task.supportsInlineDetails,
                        isExpanded: expandedTaskId == task.id,
                        isSelected: context.selectedTaskId == task.id,
                        onToggleDetails: {
                            if expandedTaskId == task.id {
                                expandedTaskId = nil
                            } else {
                                expandedTaskId = task.id
                            }
                        },
                        onSelect: {
                            context.selectedTaskId = task.id
                            context.selectedPackageId = nil
                            context.selectedUpgradePlanStepId = nil
                            if let managerId = task.managerId {
                                context.selectedManagerId = managerId
                            }
                            if !task.supportsInlineDetails {
                                expandedTaskId = nil
                            }
                        }
                    )
                }
                .listStyle(.inset)
            }
        }
        .padding(20)
        .onChange(of: core.activeTasks.map { "\($0.id):\($0.status)" }) { _ in
            collapseExpandedTaskIfNeeded()
        }
        .onChange(of: context.selectedTaskId) { selectedTaskId in
            if expandedTaskId != selectedTaskId {
                expandedTaskId = nil
            }
        }
    }

    private func collapseExpandedTaskIfNeeded() {
        guard let expandedTaskId else { return }
        let stillVisible = core.activeTasks.contains {
            $0.id == expandedTaskId && $0.supportsInlineDetails
        }
        if !stillVisible {
            self.expandedTaskId = nil
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
