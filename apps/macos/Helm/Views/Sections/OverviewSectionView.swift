import SwiftUI

struct OverviewSectionView: View {
    @EnvironmentObject private var store: AppStateStore

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                HStack {
                    Text("section.overview")
                        .font(.title2.weight(.semibold))
                    Spacer()
                    HealthBadgeView(status: store.snapshot.aggregateStatus)
                }

                HStack(spacing: 18) {
                    SummaryMetricView(titleKey: "overview.pendingUpdates", value: store.snapshot.pendingUpdates)
                    SummaryMetricView(titleKey: "overview.failures", value: store.snapshot.failures)
                    SummaryMetricView(titleKey: "overview.runningTasks", value: store.snapshot.runningTasks)
                }

                Text("overview.managerHealth")
                    .font(.headline)

                LazyVGrid(columns: [GridItem(.adaptive(minimum: 220), spacing: 12)], spacing: 12) {
                    ForEach(store.managers) { manager in
                        ManagerHealthCardView(manager: manager)
                            .onTapGesture {
                                store.selectedManagerID = manager.id
                            }
                    }
                }
            }
            .padding(20)
        }
    }
}

private struct SummaryMetricView: View {
    let titleKey: String
    let value: Int

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(LocalizedStringKey(titleKey))
                .font(.caption)
                .foregroundStyle(.secondary)
            Text("\(value)")
                .font(.title3.monospacedDigit().weight(.semibold))
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
    }
}
