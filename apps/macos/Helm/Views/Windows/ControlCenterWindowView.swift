import SwiftUI

struct ControlCenterWindowView: View {
    @EnvironmentObject private var store: AppStateStore

    var body: some View {
        NavigationSplitView {
            List(HelmSection.allCases, selection: $store.selectedSection) { section in
                Label {
                    Text(LocalizedStringKey(section.localizationKey))
                } icon: {
                    Image(systemName: section.symbolName)
                }
                .tag(Optional(section))
            }
            .navigationTitle(Text("window.controlCenter"))
        } detail: {
            HSplitView {
                SectionHostView()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                InspectorPaneView()
            }
        }
    }
}

private struct SectionHostView: View {
    @EnvironmentObject private var store: AppStateStore

    var body: some View {
        switch store.selectedSection ?? .overview {
        case .overview:
            OverviewSectionView()
        case .updates:
            UpdatesSectionView()
        case .packages:
            PackagesSectionView()
        case .tasks:
            TasksSectionView()
        case .managers:
            ManagersSectionView()
        case .settings:
            SettingsSectionView()
        }
    }
}
