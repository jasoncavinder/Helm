import SwiftUI

struct PackagesSectionView: View {
    @ObservedObject private var core = HelmCore.shared
    @EnvironmentObject private var context: ControlCenterContext
    @State private var selectedStatusFilter: PackageStatus?
    @State private var selectedManagerId: String?

    private var allPackages: [PackageItem] {
        core.allKnownPackages
    }

    private var availableManagerIds: [String] {
        Array(Set(allPackages.map(\.managerId))).sorted {
            localizedManagerDisplayName($0).localizedCaseInsensitiveCompare(localizedManagerDisplayName($1)) == .orderedAscending
        }
    }

    private var displayedPackages: [PackageItem] {
        let query = context.searchQuery.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        let managerFilter = selectedManagerId ?? context.managerFilterId

        var base = allPackages

        if !query.isEmpty {
            let localMatches = base.filter {
                $0.name.lowercased().contains(query)
                    || $0.manager.lowercased().contains(query)
            }

            let localIds = Set(localMatches.map(\.id))
            let remoteMatches = core.searchResults.filter { !localIds.contains($0.id) }
            base = (localMatches + remoteMatches)
                .sorted { lhs, rhs in
                    lhs.name.localizedCaseInsensitiveCompare(rhs.name) == .orderedAscending
                }
        }

        if let managerFilter {
            base = base.filter { $0.managerId == managerFilter }
        }

        if let statusFilter = selectedStatusFilter {
            base = base.filter { package in
                if package.status == statusFilter {
                    return true
                }
                if statusFilter == .installed && package.status == .upgradable {
                    return true
                }
                return false
            }
        }

        return base
    }

    private func kegPolicyMenuSelection(for package: PackageItem) -> KegPolicyMenuSelection? {
        guard package.managerId == "homebrew_formula", package.status != .available else {
            return nil
        }

        switch core.kegPolicySelection(for: package) {
        case .useGlobal:
            return .useGlobal
        case .keep:
            return .keep
        case .cleanup:
            return .cleanup
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text(ControlCenterSection.packages.title)
                    .font(.title2.weight(.semibold))
                Spacer()
                if core.isSearching {
                    ProgressView()
                        .controlSize(.small)
                }
            }

            if !context.searchQuery.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                HStack(spacing: 8) {
                    Image(systemName: "magnifyingglass")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Text(context.searchQuery)
                        .font(.caption)
                        .lineLimit(1)
                    Spacer()
                    Button {
                        context.searchQuery = ""
                        core.searchText = ""
                    } label: {
                        Image(systemName: "xmark.circle.fill")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    .buttonStyle(.plain)
                    .helmPointer()
                }
                .padding(.horizontal, 10)
                .padding(.vertical, 7)
                .background(
                    RoundedRectangle(cornerRadius: 8, style: .continuous)
                        .fill(Color.primary.opacity(0.05))
                )
            }

            HStack(spacing: 6) {
                ForEach(PackageStatus.allCases, id: \.self) { status in
                    FilterButton(
                        title: status.displayName,
                        isSelected: selectedStatusFilter == status,
                        action: {
                            selectedStatusFilter = (selectedStatusFilter == status) ? nil : status
                        }
                    )
                }

                Spacer()

                Menu {
                    Button(L10n.App.Packages.Filter.allManagers.localized) {
                        selectedManagerId = nil
                        context.managerFilterId = nil
                    }
                    Divider()
                    ForEach(availableManagerIds, id: \.self) { managerId in
                        Button(localizedManagerDisplayName(managerId)) {
                            selectedManagerId = managerId
                            context.managerFilterId = managerId
                        }
                    }
                } label: {
                    HStack(spacing: 4) {
                        Image(systemName: "square.stack.3d.up")
                            .font(.caption)
                        Text(managerLabel)
                            .font(.caption)
                    }
                    .padding(.horizontal, 8)
                    .padding(.vertical, 5)
                    .background((selectedManagerId ?? context.managerFilterId) == nil ? Color.gray.opacity(0.12) : Color.accentColor.opacity(0.18))
                    .cornerRadius(7)
                }
                .menuStyle(.borderlessButton)
                .helmPointer()
            }

            if displayedPackages.isEmpty {
                Spacer()
                Text(L10n.App.Packages.State.noPackagesFound.localized)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                Spacer()
            } else {
                List(displayedPackages) { package in
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
                    .contentShape(Rectangle())
                    .onTapGesture {
                        context.selectedPackageId = package.id
                        context.selectedManagerId = package.managerId
                    }
                    .helmPointer()
                }
                .listStyle(.inset)
            }
        }
        .padding(20)
        .onAppear {
            if let managerId = context.selectedManagerId {
                selectedManagerId = managerId
                context.managerFilterId = managerId
            }
            if context.searchQuery != core.searchText {
                context.searchQuery = core.searchText
            }
        }
    }

    private var managerLabel: String {
        if let selectedManagerId {
            return localizedManagerDisplayName(selectedManagerId)
        }
        if let managerFilterId = context.managerFilterId {
            return localizedManagerDisplayName(managerFilterId)
        }
        return L10n.App.Packages.Filter.allManagers.localized
    }
}

// Backward compatibility wrapper for legacy references.
struct PackageListView: View {
    @Binding var searchText: String

    var body: some View {
        PackagesSectionView()
            .onAppear {
                if !searchText.isEmpty {
                    HelmCore.shared.searchText = searchText
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
