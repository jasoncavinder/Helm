import SwiftUI

enum HelmTab: CaseIterable {
    case dashboard
    case packages
    case managers

    var title: String {
        switch self {
        case .dashboard:
            return L10n.App.Navigation.dashboard.localized
        case .packages:
            return L10n.App.Navigation.packages.localized
        case .managers:
            return L10n.App.Navigation.managers.localized
        }
    }
}

struct NavigationBarView: View {
    @Binding var selectedTab: HelmTab
    @Binding var searchText: String
    @Binding var showSettings: Bool
    @ObservedObject var core = HelmCore.shared

    var body: some View {
        VStack(spacing: 8) {
            HStack(spacing: 0) {
                ForEach(HelmTab.allCases, id: \.self) { tab in
                    Button(action: { selectedTab = tab }) {
                        Text(tab.title)
                            .font(.headline)
                            .foregroundColor(selectedTab == tab ? .primary : .secondary)
                            .padding(.vertical, 6)
                            .padding(.horizontal, 12)
                    }
                    .buttonStyle(.plain)
                }

                Spacer()

                Button(action: { showSettings.toggle() }) {
                    Image(systemName: "gearshape")
                        .font(.body)
                        .foregroundColor(.secondary)
                }
                .buttonStyle(.plain)
                .popover(isPresented: $showSettings, arrowEdge: .top) {
                    SettingsPopoverView()
                }
            }

            HStack {
                if core.isSearching {
                    ProgressView()
                        .scaleEffect(0.5)
                        .frame(width: 14, height: 14)
                } else {
                    Image(systemName: "magnifyingglass")
                        .foregroundColor(.secondary)
                        .font(.subheadline)
                }
                TextField(L10n.App.Navigation.searchPlaceholder.localized, text: $searchText)
                    .textFieldStyle(.plain)
                    .font(.subheadline)
                if !searchText.isEmpty {
                    Button(action: { searchText = "" }) {
                        Image(systemName: "xmark.circle.fill")
                            .foregroundColor(.secondary)
                            .font(.caption)
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 5)
            .background(
                RoundedRectangle(cornerRadius: 6)
                    .fill(Color.gray.opacity(0.12))
            )
        }
        .padding(.horizontal, 12)
        .padding(.top, 10)
        .padding(.bottom, 4)
    }
}
