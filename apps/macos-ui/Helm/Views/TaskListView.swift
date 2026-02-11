import SwiftUI

struct TaskListView: View {
    @StateObject var core = HelmCore.shared
    
    var body: some View {
        VStack(alignment: .leading) {
            Text("Recent Tasks")
                .font(.headline)
                .padding(.horizontal)
            
            if core.activeTasks.isEmpty {
                Text("No recent tasks")
                    .foregroundColor(.secondary)
                    .padding()
            } else {
                List(core.activeTasks) { task in
                    HStack {
                        Text(task.description)
                        Spacer()
                        Text(task.status)
                            .foregroundColor(.secondary)
                    }
                }
            }
        }
    }
}
