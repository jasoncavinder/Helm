import SwiftUI

struct PackageListView: View {
    @ObservedObject var core = HelmCore.shared
    @Binding var searchText: String
    @State private var selectedStatusFilters: Set<PackageStatus> = []
    @State private var detailsPackage: PackageItem? = nil

    private var allPackages: [PackageItem] {
        var packages = core.outdatedPackages
        let outdatedIds = Set(core.outdatedPackages.map { $0.id })
        packages.append(contentsOf: core.installedPackages.filter {
            !outdatedIds.contains($0.id)
        })
        return packages.sorted {
            $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending
        }
    }

    private var displayedPackages: [PackageItem] {
        let query = searchText.trimmingCharacters(in: .whitespaces).lowercased()

        var base: [PackageItem]
        if query.isEmpty {
            // Show all installed/outdated + cached available packages
            base = allPackages
            let existingIds = Set(base.map { $0.id })
            let available = core.cachedAvailablePackages.filter { !existingIds.contains($0.id) }
            base.append(contentsOf: available)
            base.sort {
                $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending
            }
        } else {
            // Filter installed/outdated by name match
            base = allPackages.filter { $0.name.lowercased().contains(query) }

            // Merge in search results (deduplicated by ID)
            let existingIds = Set(base.map { $0.id })
            let newResults = core.searchResults.filter { !existingIds.contains($0.id) }
            base.append(contentsOf: newResults)
        }

        if selectedStatusFilters.isEmpty {
            return base
        }
        return base.filter { selectedStatusFilters.contains($0.status) }
    }

    var body: some View {
        VStack(spacing: 0) {
            // Connection banner
            if !core.isConnected {
                HStack {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundColor(.yellow)
                    Text("Reconnecting to service...")
                        .font(.caption)
                    Spacer()
                }
                .padding(8)
                .background(Color.yellow.opacity(0.15))
            }

            // Filter bar
            HStack(spacing: 4) {
                ForEach(PackageStatus.allCases, id: \.self) { status in
                    FilterButton(
                        title: status.displayName,
                        isSelected: selectedStatusFilters.contains(status),
                        action: {
                            if selectedStatusFilters.contains(status) {
                                selectedStatusFilters.remove(status)
                            } else {
                                selectedStatusFilters.insert(status)
                            }
                        }
                    )
                }
                Spacer()
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 6)

            Divider()

            // Package list
            if displayedPackages.isEmpty {
                VStack {
                    Spacer()
                    Text("No packages found")
                        .foregroundColor(.secondary)
                        .font(.subheadline)
                    Spacer()
                }
            } else {
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(displayedPackages) { package in
                            PackageRowView(package: package)
                                .contentShape(Rectangle())
                                .onTapGesture {
                                    detailsPackage = package
                                }
                            Divider()
                                .padding(.leading, 36)
                        }
                    }
                }
            }
        }
        .popover(item: $detailsPackage) { package in
            PackageDetailPopover(package: package)
                .frame(width: 250)
        }
    }
}

private struct PackageDetailPopover: View {
    let package: PackageItem

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(package.name)
                .font(.headline)

            LabeledContentRow(label: "Manager", value: package.manager)
            LabeledContentRow(label: "Version", value: package.version)

            if let latest = package.latestVersion {
                LabeledContentRow(label: "Available", value: latest, valueColor: .orange)
            }

            Divider()

            HStack {
                if package.status == .upgradable {
                    Button("Upgrade") {}
                        .disabled(true)
                        .help("Upgrade not yet implemented")
                }

                Button("Uninstall") {}
                    .disabled(true)
                    .help("Uninstall not yet implemented")
            }
            .padding(.top, 4)
        }
        .padding(12)
    }
}

extension PackageItem: Hashable {
    static func == (lhs: PackageItem, rhs: PackageItem) -> Bool {
        lhs.id == rhs.id
    }

    func hash(into hasher: inout Hasher) {
        hasher.combine(id)
    }
}
