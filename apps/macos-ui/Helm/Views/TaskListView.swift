import SwiftUI

struct TaskListView: View {
    @ObservedObject var core = HelmCore.shared
    var maxTasks: Int = 10

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            if core.activeTasks.isEmpty {
                Text(L10n.App.Tasks.noRecentTasks.localized)
                    .foregroundColor(.secondary)
                    .font(.caption)
                    .padding(.horizontal, 16)
            } else {
                ForEach(Array(core.activeTasks.prefix(maxTasks))) { task in
                    TaskRowView(task: task)
                }
            }
        }
    }
}
