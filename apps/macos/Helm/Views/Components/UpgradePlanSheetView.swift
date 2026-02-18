import SwiftUI

struct UpgradePlanSheetView: View {
    @EnvironmentObject private var store: AppStateStore

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("updates.executionPlan")
                .font(.title3.weight(.semibold))

            ForEach(store.executionStages) { stage in
                HStack {
                    Text(LocalizedStringKey(stage.authority.localizationKey))
                    Spacer()
                    Text("\(stage.managerCount)")
                        .font(.callout.monospacedDigit())
                    Text("updates.managers")
                        .foregroundStyle(.secondary)
                    Text("\(stage.packageCount)")
                        .font(.callout.monospacedDigit())
                    Text("updates.packages")
                        .foregroundStyle(.secondary)
                }
                .font(.callout)
            }

            Divider()

            HStack {
                Button("action.cancel") {
                    store.isShowingUpgradeSheet = false
                }
                Spacer()
                Button("action.dryRun") {
                    store.runUpgradeAll(dryRun: true)
                }
                Button("action.runUpgradePlan") {
                    store.runUpgradeAll(dryRun: false)
                }
                .keyboardShortcut(.defaultAction)
            }
        }
        .padding(20)
        .frame(minWidth: 480)
    }
}
