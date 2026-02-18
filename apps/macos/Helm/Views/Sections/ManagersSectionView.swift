import SwiftUI

struct ManagersSectionView: View {
    @EnvironmentObject private var store: AppStateStore

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                Text("section.managers")
                    .font(.title2.weight(.semibold))

                ForEach(store.managers) { manager in
                    ManagerHealthCardView(manager: manager)
                        .onTapGesture {
                            store.selectedManagerID = manager.id
                            store.selectedPackageID = nil
                        }
                }
            }
            .padding(20)
        }
    }
}
