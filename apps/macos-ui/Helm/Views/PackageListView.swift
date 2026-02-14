import SwiftUI

struct PackageListView: View {
    @ObservedObject var core = HelmCore.shared
    @Binding var searchText: String
    @State private var selectedStatusFilter: PackageStatus? = nil
    @State private var selectedManager: String? = nil

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

    private var availableManagers: [String] {
        Array(Set(allPackages.map { $0.manager })).sorted()
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

        if let manager = selectedManager {
            base = base.filter { $0.manager == manager }
        }

        guard let filter = selectedStatusFilter else {
            return base
        }
        return base.filter { pkg in
            if pkg.status == filter {
                return true
            }
            // "Installed" filter should also include upgradable packages
            if filter == .installed && pkg.status == .upgradable {
                return true
            }
            return false
        }
    }

    private func kegPolicyMenuSelection(for package: PackageItem) -> KegPolicyMenuSelection? {
        guard package.managerId == "homebrew_formula", package.status != .available else {
            return nil
        }

        let selection = core.kegPolicySelection(for: package)
        switch selection {
        case .useGlobal:
            return .useGlobal
        case .keep:
            return .keep
        case .cleanup:
            return .cleanup
        }
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
                        isSelected: selectedStatusFilter == status,
                        action: {
                            if selectedStatusFilter == status {
                                selectedStatusFilter = nil
                            } else {
                                selectedStatusFilter = status
                            }
                        }
                    )
                }
                Spacer()

                // Manager Filter
                Menu {
                    Button("All Managers") { selectedManager = nil }
                    Divider()
                    ForEach(availableManagers, id: \.self) { manager in
                        Button(manager) { selectedManager = manager }
                    }
                } label: {
                    HStack(spacing: 3) {
                        Image(systemName: "square.stack.3d.up")
                            .imageScale(.small)
                        Text(selectedManager ?? "All Managers")
                    }
                    .font(.caption2)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 3)
                    .background(selectedManager != nil ? Color.accentColor.opacity(0.2) : Color.gray.opacity(0.1))
                    .cornerRadius(5)
                    .foregroundColor(selectedManager != nil ? .accentColor : .primary)
                }
                .menuStyle(.borderlessButton)
                .fixedSize()
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
                            PackageRowView(
                                package: package,
                                isPinActionInFlight: core.pinActionPackageIds.contains(package.id),
                                isUpgradeActionInFlight: core.upgradeActionPackageIds.contains(package.id),
                                kegPolicySelection: kegPolicyMenuSelection(for: package),
                                onSelectKegPolicy: package.managerId == "homebrew_formula"
                                    ? { selection in
                                        switch selection {
                                        case .useGlobal:
                                            core.setKegPolicySelection(for: package, selection: .useGlobal)
                                        case .keep:
                                            core.setKegPolicySelection(for: package, selection: .keep)
                                        case .cleanup:
                                            core.setKegPolicySelection(for: package, selection: .cleanup)
                                        }
                                    }
                                    : nil,
                                onUpgrade: core.canUpgradeIndividually(package)
                                    ? { core.upgradePackage(package) }
                                    : nil,
                                onTogglePin: package.status == .available
                                    ? nil
                                    : {
                                        if package.pinned {
                                            core.unpinPackage(package)
                                        } else {
                                            core.pinPackage(package)
                                        }
                                    }
                            )
                            Divider()
                                .padding(.leading, 36)
                        }
                    }
                }
            }
        }
        .onAppear {
            if let filter = core.selectedManagerFilter {
                selectedManager = filter
                core.selectedManagerFilter = nil
            }
        }
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
