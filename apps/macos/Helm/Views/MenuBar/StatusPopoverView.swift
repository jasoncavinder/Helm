import SwiftUI

struct StatusPopoverView: View {
    @Environment(\.openWindow) private var openWindow
    @EnvironmentObject private var store: AppStateStore

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            VStack(alignment: .leading, spacing: 8) {
                Text("app.name")
                    .font(.headline)
                HStack(spacing: 8) {
                    HealthBadgeView(status: store.snapshot.aggregateStatus)
                    Text("\(store.snapshot.pendingUpdates)")
                        .font(.subheadline.monospacedDigit())
                    Text("popover.pendingUpdates")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }
            }

            HStack {
                Button("action.refresh") {
                    store.refresh()
                }
                Button("action.upgradeAll") {
                    store.isShowingUpgradeSheet = true
                }
            }

            VStack(alignment: .leading, spacing: 8) {
                Text("popover.managerSnapshot")
                    .font(.subheadline.weight(.semibold))
                ForEach(store.managers.prefix(4)) { manager in
                    HStack {
                        Text(manager.displayName)
                        Spacer()
                        Text("\(manager.outdatedCount)")
                            .font(.caption.monospacedDigit())
                        HealthBadgeView(status: manager.status)
                    }
                    .font(.caption)
                }
            }

            VStack(alignment: .leading, spacing: 8) {
                Text("popover.activeTasks")
                    .font(.subheadline.weight(.semibold))
                ForEach(store.tasks.prefix(2)) { task in
                    TaskRowView(task: task)
                }
            }

            Button("action.openControlCenter") {
                openWindow(id: "control-center")
            }
            .buttonStyle(.borderedProminent)
            .frame(maxWidth: .infinity, alignment: .trailing)
        }
        .padding(16)
        .frame(width: 380)
        .sheet(isPresented: $store.isShowingUpgradeSheet) {
            UpgradePlanSheetView()
                .environmentObject(store)
        }
    }
}
