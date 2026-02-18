import SwiftUI

struct PackagesSectionView: View {
    @EnvironmentObject private var store: AppStateStore

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("section.packages")
                .font(.title2.weight(.semibold))

            TextField(LocalizedStringKey("search.placeholder"), text: $store.searchQuery)
                .textFieldStyle(.roundedBorder)

            List(store.visiblePackages) { package in
                HStack(spacing: 12) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(package.name)
                            .font(.body.weight(.medium))
                        HStack(spacing: 6) {
                            Text(package.managerDisplayName)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                            Text("dot.separator")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                            Text(package.installedVersion)
                                .font(.caption.monospaced())
                            Text("arrow.right")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                            Text(package.latestVersion)
                                .font(.caption.monospaced())
                        }
                    }

                    Spacer()

                    if package.hasUpdate {
                        Button("action.update") {
                            store.update(package: package)
                        }
                    }

                    Button(package.isPinned ? "action.unpin" : "action.pin") {
                        store.togglePin(packageID: package.id)
                    }
                }
                .contentShape(Rectangle())
                .onTapGesture {
                    store.selectedPackageID = package.id
                    store.selectedManagerID = package.managerID
                }
            }
            .listStyle(.inset)
        }
        .padding(20)
    }
}
