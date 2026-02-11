import SwiftUI

struct TaskListView: View {
    @State private var tasks: [TaskItem] = []
    
    var body: some View {
        VStack(alignment: .leading) {
            Text("Active Tasks")
                .font(.headline)
                .padding(.horizontal)
            
            if tasks.isEmpty {
                Text("No active tasks")
                    .foregroundColor(.secondary)
                    .padding()
            } else {
                List(tasks) { task in
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
