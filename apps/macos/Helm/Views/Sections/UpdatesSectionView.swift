import SwiftUI

struct UpdatesSectionView: View {
    @EnvironmentObject private var store: AppStateStore

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack {
                Text("section.updates")
                    .font(.title2.weight(.semibold))
                Spacer()
                Button("action.refreshPlan") {
                    store.refresh()
                }
            }

            Text("updates.executionPlan")
                .font(.headline)

            ForEach(store.executionStages) { stage in
                HStack {
                    Text(LocalizedStringKey(stage.authority.localizationKey))
                    Spacer()
                    Text("\(stage.managerCount)")
                        .font(.body.monospacedDigit())
                    Text("updates.managers")
                        .foregroundStyle(.secondary)
                    Text("\(stage.packageCount)")
                        .font(.body.monospacedDigit())
                    Text("updates.packages")
                        .foregroundStyle(.secondary)
                }
                .padding(.vertical, 4)
            }

            Divider()

            HStack {
                Button("action.dryRun") {
                    store.runUpgradeAll(dryRun: true)
                }
                Button("action.runUpgradePlan") {
                    store.isShowingUpgradeSheet = true
                }
                .keyboardShortcut(.defaultAction)
            }

            Spacer()
        }
        .padding(20)
        .sheet(isPresented: $store.isShowingUpgradeSheet) {
            UpgradePlanSheetView()
                .environmentObject(store)
        }
    }
}
