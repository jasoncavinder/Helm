import SwiftUI

struct TasksSectionView: View {
    @EnvironmentObject private var store: AppStateStore

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("section.tasks")
                .font(.title2.weight(.semibold))

            List(store.tasks) { task in
                TaskRowView(task: task)
            }
            .listStyle(.inset)

            Spacer()
        }
        .padding(20)
    }
}
