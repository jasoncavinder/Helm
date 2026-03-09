import SwiftUI

struct PackagesSectionView: View {
    @ObservedObject private var core = HelmCore.shared
    @EnvironmentObject private var context: ControlCenterContext
    @State private var selectedStatusFilter: PackageStatus?
    @State private var showPinnedOnly = false
    @State private var selectedManagerId: String?
    @State private var showInstallManagerSheet = false
    @State private var installSelectionRow: ConsolidatedPackageItem?
    @State private var selectedInstallManagerId: String?
    @State private var selectedInstallPackageId: String?
    @State private var availableManagerIds: [String] = []
    @State private var displayedPackages: [ConsolidatedPackageItem] = []
    @State private var installableAvailablePackageNames: Set<String> = []
    @State private var installActionPackageNames: Set<String> = []

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

            HStack(spacing: 8) {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 6) {
                        ForEach(PackageStatus.allCases, id: \.self) { status in
                            FilterButton(
                                title: status.displayName,
                                isSelected: selectedStatusFilter == status,
                                action: {
                                    selectedStatusFilter = (selectedStatusFilter == status) ? nil : status
                                    showPinnedOnly = false
                                }
                            )
                        }

                        FilterButton(
                            title: L10n.App.Packages.Filter.pinned.localized,
                            isSelected: showPinnedOnly,
                            action: {
                                showPinnedOnly.toggle()
                                if showPinnedOnly {
                                    selectedStatusFilter = nil
                                }
                            }
                        )
                    }
                    .padding(.vertical, 1)
                }
                .frame(maxWidth: .infinity, alignment: .leading)

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
                Text(L10n.App.Packages.State.noPackagesFound.localized)
                    .font(.subheadline)
                    .foregroundColor(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.top, 4)
                Spacer()
            } else {
                let rows = displayedPackages
                let activeManagerFilterId = selectedManagerId ?? context.managerFilterId
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 6) {
                        ForEach(rows) { packageRow in
                            let preferredManagerId = activeManagerFilterId
                                ?? core.preferredManagerId(for: packageRow.package)
                            let selectedPackageId = packageRow.containsPackageId(context.selectedPackageId)
                                ? context.selectedPackageId
                                : nil
                            let package = packageRow.actionTarget(
                                preferredManagerId: preferredManagerId,
                                selectedPackageId: selectedPackageId
                            )
                            let primaryAction = primaryPackageAction(
                                for: packageRow,
                                actionTarget: package,
                                managerConstraint: activeManagerFilterId
                            )
                            HStack(spacing: 8) {
                                PackageRowView(
                                    package: package,
                                    managerDisplayNames: packageRow.managerDisplayNames,
                                    detailBadges: rowDetailBadges(for: packageRow, actionTarget: package),
                                    isSelected: packageRow.containsPackageId(context.selectedPackageId)
                                )
                                .id("\(package.id)|\(package.pinned ? 1 : 0)")
                                .contentShape(Rectangle())
                                .onTapGesture {
                                    context.selectedPackageId = package.id
                                    context.selectedManagerId = package.managerId
                                    context.selectedTaskId = nil
                                    context.selectedUpgradePlanStepId = nil
                                }
                                .helmPointer()

                                primaryActionButton(for: primaryAction)
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(.trailing, 8)
                        }
                    }
                    .padding(.vertical, 4)
                }
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
            refreshPackageSnapshots()
            if normalizeManagerSelection() {
                refreshPackageSnapshots()
            }
        }
        .onChange(of: core.managerStatuses.mapValues(\.enabled)) { _ in
            refreshPackageSnapshots()
            if normalizeManagerSelection() {
                refreshPackageSnapshots()
            }
        }
        .onChange(of: availableManagerIds) { _ in
            if normalizeManagerSelection() {
                refreshPackageSnapshots()
            }
        }
        .onReceive(core.$installedPackages) { _ in refreshPackageSnapshots() }
        .onReceive(core.$outdatedPackages) { _ in refreshPackageSnapshots() }
        .onReceive(core.$cachedAvailablePackages) { _ in refreshPackageSnapshots() }
        .onReceive(core.$searchResults) { _ in
            let hasQuery = !context.searchQuery.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            if hasQuery {
                refreshPackageSnapshots()
            }
        }
        .onChange(of: core.installActionPackageIds) { _ in refreshPackageSnapshots() }
        .onChange(of: core.pinActionPackageIds) { _ in refreshPackageSnapshots() }
        .onChange(of: context.searchQuery) { _ in refreshPackageSnapshots() }
        .onChange(of: selectedStatusFilter) { _ in refreshPackageSnapshots() }
        .onChange(of: showPinnedOnly) { _ in refreshPackageSnapshots() }
        .onChange(of: selectedManagerId) { _ in refreshPackageSnapshots() }
        .onChange(of: context.managerFilterId) { _ in refreshPackageSnapshots() }
        .sheet(isPresented: $showInstallManagerSheet) {
            installManagerSheet
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

    private func normalizeManagerSelection() -> Bool {
        var changed = false
        if let selectedManagerId, !availableManagerIds.contains(selectedManagerId) {
            self.selectedManagerId = nil
            changed = true
        }
        if let managerFilterId = context.managerFilterId, !availableManagerIds.contains(managerFilterId) {
            context.managerFilterId = nil
            changed = true
        }
        return changed
    }

    private func refreshPackageSnapshots() {
        let allPackages = core.allKnownPackages
        let candidateSourcePackages = mergeCandidatePackages(
            primary: allPackages,
            secondary: core.searchResults
        )
        availableManagerIds = Array(Set(candidateSourcePackages.map(\.managerId))).sorted {
            localizedManagerDisplayName($0).localizedCaseInsensitiveCompare(localizedManagerDisplayName($1)) == .orderedAscending
        }
        var installableNames = Set<String>()
        for package in candidateSourcePackages {
            let normalizedName = normalizedPackageIdentity(package)
            if package.status == .available, core.canInstallPackage(package, includeAlternates: false) {
                installableNames.insert(normalizedName)
            }
        }
        installableAvailablePackageNames = installableNames
        installActionPackageNames = core.installActionInFlightPackageNames(knownPackages: allPackages)
        displayedPackages = core.filteredPackages(
            query: context.searchQuery,
            managerId: selectedManagerId ?? context.managerFilterId,
            statusFilter: selectedStatusFilter,
            pinnedOnly: showPinnedOnly,
            knownPackages: allPackages
        )
    }

    private func mergeCandidatePackages(
        primary: [PackageItem],
        secondary: [PackageItem]
    ) -> [PackageItem] {
        var mergedById = Dictionary(uniqueKeysWithValues: primary.map { ($0.id, $0) })
        for candidate in secondary {
            if var existing = mergedById[candidate.id] {
                if existing.summary?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty != false,
                   let summary = candidate.summary?.trimmingCharacters(in: .whitespacesAndNewlines),
                   !summary.isEmpty {
                    existing.summary = summary
                }
                if existing.latestVersion == nil {
                    existing.latestVersion = candidate.latestVersion
                }
                existing.restartRequired = existing.restartRequired || candidate.restartRequired
                mergedById[candidate.id] = existing
            } else {
                mergedById[candidate.id] = candidate
            }
        }
        return Array(mergedById.values)
    }

    private func normalizedPackageIdentity(_ package: PackageItem) -> String {
        PackageActionTracking.normalizedPackageIdentityKey(
            name: package.name,
            version: package.version
        )
    }

    private func primaryPackageAction(
        for packageRow: ConsolidatedPackageItem,
        actionTarget package: PackageItem,
        managerConstraint: String?
    ) -> PrimaryPackageAction {
        if package.pinned, core.canPinPackage(package) {
            let inFlight = core.pinActionPackageIds.contains(package.id)
            return PrimaryPackageAction(
                symbol: "pin.slash",
                tooltip: L10n.App.Packages.Action.unpin.localized,
                enabled: !inFlight,
                inFlight: inFlight,
                action: { core.unpinPackage(package) }
            )
        }

        if package.status == .available {
            let packageName = normalizedPackageIdentity(package)
            let inFlight = installActionPackageNames.contains(packageName)
            let canInstall = managerConstraint == nil
                ? installableAvailablePackageNames.contains(packageName)
                : core.canInstallPackage(package, includeAlternates: false)
            return PrimaryPackageAction(
                symbol: "arrow.down.circle",
                tooltip: L10n.App.Packages.Action.install.localized,
                enabled: canInstall && !inFlight,
                inFlight: inFlight,
                action: { startInstallAction(for: packageRow, managerConstraint: managerConstraint) }
            )
        }

        let inFlight = core.upgradeActionPackageIds.contains(package.id)
        let canUpgrade = core.canUpgradeIndividually(package)
        return PrimaryPackageAction(
            symbol: "arrow.up.circle",
            tooltip: L10n.App.Packages.Action.upgradePackage.localized,
            enabled: canUpgrade && !inFlight,
            inFlight: inFlight,
            action: { core.upgradePackage(package) }
        )
    }

    private func rowDetailBadges(
        for packageRow: ConsolidatedPackageItem,
        actionTarget package: PackageItem
    ) -> [String] {
        let managerPackages = packageRow.packages(forManagerId: package.managerId)
        guard !managerPackages.isEmpty else { return [] }

        var badges: [String] = []
        let distinctVersions = Set(
            managerPackages.compactMap { candidate -> String? in
                let normalizedVersion = candidate.version.trimmingCharacters(in: .whitespacesAndNewlines)
                guard PackageIdentity.hasKnownVersion(normalizedVersion) else { return nil }
                return normalizedVersion
            }
        )
        let versionCount = distinctVersions.isEmpty ? managerPackages.count : distinctVersions.count
        if versionCount > 1 {
            badges.append(
                L10n.App.Packages.Label.versionCount.localized(with: [
                    "count": "\(versionCount)"
                ])
            )
        }
        if managerPackages.contains(where: { $0.runtimeState.isActive }) {
            badges.append(L10n.App.Inspector.packageRuntimeStateActive.localized)
        }
        if managerPackages.contains(where: { $0.runtimeState.isDefault }) {
            badges.append(L10n.App.Inspector.packageRuntimeStateDefault.localized)
        }
        if managerPackages.contains(where: { $0.runtimeState.hasOverride }) {
            badges.append(L10n.App.Inspector.packageRuntimeStateOverride.localized)
        }
        return badges
    }

    private func primaryActionButton(for action: PrimaryPackageAction) -> some View {
        Button(action: { action.action?() }) {
            Image(systemName: action.symbol)
        }
        .buttonStyle(HelmIconButtonStyle())
        .help(action.tooltip)
        .accessibilityLabel(action.tooltip)
        .disabled(!action.enabled || action.inFlight)
        .helmPointer(enabled: action.enabled && !action.inFlight)
        .padding(.trailing, 4)
    }

    private var installSelectionCandidates: [PackageItem] {
        guard let installSelectionRow else { return [] }
        let managerConstraint = selectedInstallManagerId?.trimmingCharacters(in: .whitespacesAndNewlines)
        return installSelectionRow.memberPackages.filter {
            guard $0.status == .available else { return false }
            guard core.canInstallPackage($0, includeAlternates: false) else { return false }
            guard let managerConstraint, !managerConstraint.isEmpty else { return true }
            return $0.managerId == managerConstraint
        }
    }

    private var selectedInstallCandidate: PackageItem? {
        if let selectedInstallPackageId,
           let matched = installSelectionCandidates.first(where: { $0.id == selectedInstallPackageId }) {
            return matched
        }
        return installSelectionCandidates.first
    }

    private var installSelectionManagerIds: [String] {
        let managerIds = installSelectionRow?.memberPackages.compactMap { candidate -> String? in
            guard candidate.status == .available,
                  core.canInstallPackage(candidate, includeAlternates: false) else {
                return nil
            }
            return candidate.managerId
        } ?? []
        return PackageConsolidationPolicy.sortedManagerIds(
            managerIds,
            localizedManagerName: localizedManagerDisplayName,
            priorityRank: { core.managerPriorityRank(for: $0) }
        )
    }

    private var installSelectionMembersForSelectedManager: [PackageItem] {
        let managerId = selectedInstallManagerId?.trimmingCharacters(in: .whitespacesAndNewlines)
        return installSelectionCandidates.filter {
            guard let managerId, !managerId.isEmpty else { return true }
            return $0.managerId == managerId
        }
    }

    private var installManagerSheet: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(
                "\(L10n.App.Packages.Action.install.localized) \(installSelectionRow?.package.displayName ?? "")"
            )
            .font(.headline)

            Picker(
                L10n.App.Inspector.manager.localized,
                selection: Binding(
                    get: { selectedInstallManagerId ?? installSelectionManagerIds.first ?? "" },
                    set: { newValue in
                        selectedInstallManagerId = newValue
                        selectedInstallPackageId = installSelectionMembersForManager(newValue).first?.id
                    }
                )
            ) {
                ForEach(installSelectionManagerIds, id: \.self) { managerId in
                    Text(localizedManagerDisplayName(managerId))
                        .tag(managerId)
                }
            }
            .pickerStyle(.radioGroup)

            if installSelectionMembersForSelectedManager.count > 1 {
                Picker(
                    L10n.App.Inspector.version.localized,
                    selection: Binding(
                        get: { selectedInstallPackageId ?? installSelectionMembersForSelectedManager.first?.id ?? "" },
                        set: { selectedInstallPackageId = $0 }
                    )
                ) {
                    ForEach(installSelectionMembersForSelectedManager, id: \.id) { candidate in
                        Text(installSelectionLabel(for: candidate))
                            .tag(candidate.id)
                    }
                }
                .pickerStyle(.radioGroup)
            }

            HStack(spacing: 8) {
                Spacer()
                Button(L10n.Common.cancel.localized) {
                    dismissInstallManagerSheet()
                }
                Button(L10n.Common.install.localized) {
                    guard let selectedInstallCandidate else { return }
                    dismissInstallManagerSheet()
                    core.installPackage(selectedInstallCandidate)
                }
                .keyboardShortcut(.defaultAction)
                .disabled(selectedInstallCandidate == nil)
            }
        }
        .padding(18)
        .frame(width: 320)
    }

    private func startInstallAction(
        for packageRow: ConsolidatedPackageItem,
        managerConstraint: String? = nil
    ) {
        let candidates = packageRow.memberPackages.filter {
            guard $0.status == .available else { return false }
            guard core.canInstallPackage($0, includeAlternates: false) else { return false }
            guard let managerConstraint, !managerConstraint.isEmpty else { return true }
            return $0.managerId == managerConstraint
        }
        guard !candidates.isEmpty else { return }
        if candidates.count == 1, let candidate = candidates.first {
            core.installPackage(candidate)
            return
        }
        installSelectionRow = packageRow
        selectedInstallManagerId = PackageConsolidationPolicy.preferredManagerId(
            managerIds: candidates.map(\.managerId),
            preferredManagerId: managerConstraint ?? core.preferredManagerId(for: packageRow.package)
        ) ?? candidates.first?.managerId
        let preferredPackageId = packageRow.containsPackageId(context.selectedPackageId)
            ? context.selectedPackageId
            : nil
        let selectedManagerMembers = installSelectionMembersForManager(selectedInstallManagerId)
        selectedInstallPackageId = selectedManagerMembers.first(where: { $0.id == preferredPackageId })?.id
            ?? selectedManagerMembers.first?.id
        showInstallManagerSheet = true
    }

    private func dismissInstallManagerSheet() {
        showInstallManagerSheet = false
        installSelectionRow = nil
        selectedInstallManagerId = nil
        selectedInstallPackageId = nil
    }

    private func installSelectionMembersForManager(_ managerId: String?) -> [PackageItem] {
        let trimmedManagerId = managerId?.trimmingCharacters(in: .whitespacesAndNewlines)
        return installSelectionRow?.memberPackages.filter {
            guard $0.status == .available else { return false }
            guard core.canInstallPackage($0, includeAlternates: false) else { return false }
            guard let trimmedManagerId, !trimmedManagerId.isEmpty else { return true }
            return $0.managerId == trimmedManagerId
        } ?? []
    }

    private func installSelectionLabel(for candidate: PackageItem) -> String {
        let normalizedVersion = candidate.version.trimmingCharacters(in: .whitespacesAndNewlines)
        if !normalizedVersion.isEmpty {
            return normalizedVersion
        }
        return candidate.displayName
    }
}

private struct PrimaryPackageAction {
    let symbol: String
    let tooltip: String
    let enabled: Bool
    let inFlight: Bool
    let action: (() -> Void)?
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
