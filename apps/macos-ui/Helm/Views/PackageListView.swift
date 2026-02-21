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
        core.filteredPackages(
            query: context.searchQuery,
            managerId: selectedManagerId ?? context.managerFilterId,
            statusFilter: selectedStatusFilter
        )
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
                    .background(
                        RoundedRectangle(cornerRadius: 7, style: .continuous)
                            .fill((selectedManagerId ?? context.managerFilterId) == nil ? HelmTheme.surfaceElevated : HelmTheme.selectionFill)
                            .overlay(
                                RoundedRectangle(cornerRadius: 7, style: .continuous)
                                    .strokeBorder(
                                        (selectedManagerId ?? context.managerFilterId) == nil
                                            ? HelmTheme.borderSubtle.opacity(0.85)
                                            : HelmTheme.selectionStroke,
                                        lineWidth: 0.8
                                    )
                            )
                    )
                }
                .menuStyle(.borderlessButton)
                .helmPointer()
                .accessibilityLabel(managerLabel)
            }

            if displayedPackages.isEmpty {
                Spacer()
                Text(L10n.App.Packages.State.noPackagesFound.localized)
                    .font(.subheadline)
                    .foregroundColor(.secondary)
                Spacer()
            } else {
                List(displayedPackages) { package in
                    PackageRowView(
                        package: package,
                        isSelected: context.selectedPackageId == package.id,
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
                        context.selectedTaskId = nil
                        context.selectedUpgradePlanStepId = nil
                    }
                    .listRowBackground(Color.clear)
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
