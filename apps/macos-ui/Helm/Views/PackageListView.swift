import SwiftUI

struct PackagesSectionView: View {
    @ObservedObject private var core = HelmCore.shared
    @EnvironmentObject private var context: ControlCenterContext
    private let researchLibraryProjection = WholeWorkflowResearchDatasetProvider.activeLibraryProjection()
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
    @ViewBuilder
    var body: some View {
        Group {
            if let researchLibraryProjection {
                researchBody(researchLibraryProjection)
            } else {
                productionBody
            }
        }
        .onAppear {
            preparePendingPackageFocusRequest()
        }
        .onChange(of: context.pendingLibraryPackageFocusRequest) { _ in
            preparePendingPackageFocusRequest()
        }
        .onChange(of: focusablePackageRowIDs) { _ in
            preparePendingPackageFocusRequest()
        }
    }

    private var productionBody: some View {
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
                libraryTable(rows: productionLibraryTableRows)
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

    private func researchBody(
        _ projection: WholeWorkflowResearchLibraryProjection
    ) -> some View {
        let results = researchResults(in: projection)
        return VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .firstTextBaseline) {
                VStack(alignment: .leading, spacing: 3) {
                    Text(ControlCenterSection.packages.title)
                        .font(.title2.weight(.semibold))
                    Text(L10n.App.Packages.Research.librarySubtitle.localized)
                        .font(.callout)
                        .foregroundColor(HelmTheme.textSecondary)
                }
                Spacer()
                if researchSearchIsEnriching(projection) {
                    HStack(spacing: 6) {
                        ProgressView()
                            .controlSize(.small)
                            .accessibilityHidden(true)
                        Text(L10n.App.Packages.Research.remoteSearchInProgress.localized)
                            .font(.caption)
                            .foregroundColor(HelmTheme.textSecondary)
                    }
                } else if projection.isOfflineVariant,
                          !context.searchQuery.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                    Label(
                        L10n.App.Packages.Research.offlineDeferred.localized,
                        systemImage: "wifi.slash"
                    )
                    .font(.caption.weight(.medium))
                    .foregroundColor(HelmTheme.stateUnavailable)
                }
            }

            researchFilterBar(projection)

            if context.searchQuery.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                VStack(spacing: 8) {
                    Image(systemName: "magnifyingglass")
                        .font(.system(size: 28, weight: .medium))
                        .foregroundColor(HelmTheme.textSecondary)
                    Text(L10n.App.Packages.Research.searchPrompt.localized)
                        .font(.headline)
                    Text(L10n.App.Packages.Research.searchPromptDetail.localized)
                        .font(.callout)
                        .foregroundColor(HelmTheme.textSecondary)
                        .multilineTextAlignment(.center)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if results.isEmpty {
                Text(L10n.App.Packages.State.noPackagesFound.localized)
                    .font(.subheadline)
                    .foregroundColor(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.top, 4)
                Spacer()
            } else {
                libraryTable(rows: researchLibraryTableRows(results, in: projection))
            }
        }
        .padding(20)
        .onAppear {
            context.updateResearchSearchPresentation(
                query: context.searchQuery,
                isOfflineVariant: projection.isOfflineVariant
            )
            reconcileResearchPresentation(projection)
        }
        .onChange(of: context.searchQuery) { _ in
            reconcileResearchPresentation(projection)
        }
        .onChange(of: context.researchRemoteSearchResultsAvailable) { _ in
            reconcileResearchPresentation(projection)
        }
    }

    private func researchFilterBar(
        _ projection: WholeWorkflowResearchLibraryProjection
    ) -> some View {
        let managerIDs = researchVisibleManagerIDs(in: projection)
        return HStack(spacing: 8) {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 6) {
                    ForEach(PackageStatus.allCases, id: \.self) { status in
                        FilterButton(
                            title: status.displayName,
                            isSelected: selectedStatusFilter == status,
                            action: {
                                selectedStatusFilter = selectedStatusFilter == status ? nil : status
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
                ForEach(managerIDs, id: \.self) { managerID in
                    Button(localizedManagerDisplayName(managerID)) {
                        selectedManagerId = managerID
                        context.managerFilterId = managerID
                    }
                }
            } label: {
                Label(managerLabel, systemImage: "square.stack.3d.up")
                    .font(.caption)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 5)
                    .background(
                        RoundedRectangle(cornerRadius: 7, style: .continuous)
                            .fill(
                                (selectedManagerId ?? context.managerFilterId) == nil
                                    ? HelmTheme.surfaceElevated
                                    : HelmTheme.selectionFill
                            )
                    )
            }
            .menuStyle(.borderlessButton)
            .helmPointer()
            .accessibilityLabel(managerLabel)
        }
    }

    private func researchResults(
        in projection: WholeWorkflowResearchLibraryProjection
    ) -> [WholeWorkflowResearchLibraryResult] {
        guard !showPinnedOnly,
              selectedStatusFilter == nil || selectedStatusFilter == .available else {
            return []
        }
        return projection.visibleResults(
            matching: context.searchQuery,
            managerID: selectedManagerId ?? context.managerFilterId,
            includeRemoteResults: context.researchRemoteSearchResultsAvailable
        )
    }

    private func researchPresentationResults(
        in projection: WholeWorkflowResearchLibraryProjection
    ) -> [WholeWorkflowResearchLibraryResult] {
        projection.visibleResults(
            matching: context.searchQuery,
            includeRemoteResults: context.researchRemoteSearchResultsAvailable
        )
    }

    private func researchVisibleManagerIDs(
        in projection: WholeWorkflowResearchLibraryProjection
    ) -> [String] {
        Array(Set(researchPresentationResults(in: projection).map(\.managerID))).sorted {
            localizedManagerDisplayName($0).localizedCaseInsensitiveCompare(
                localizedManagerDisplayName($1)
            ) == .orderedAscending
        }
    }

    private func reconcileResearchPresentation(
        _ projection: WholeWorkflowResearchLibraryProjection
    ) {
        let visibleResults = researchPresentationResults(in: projection)
        let visibleResultIDs = Set(visibleResults.map(\.id))
        if let selectedPackageID = context.selectedPackageId,
           projection.result(withID: selectedPackageID) != nil,
           !visibleResultIDs.contains(selectedPackageID) {
            context.selectedPackageId = nil
            context.selectedManagerId = nil
        }

        let visibleManagerIDs = Set(visibleResults.map(\.managerID))
        if let selectedManagerId, !visibleManagerIDs.contains(selectedManagerId) {
            self.selectedManagerId = nil
        }
        if let managerFilterID = context.managerFilterId,
           !visibleManagerIDs.contains(managerFilterID) {
            context.managerFilterId = nil
        }
    }

    private func researchSearchIsEnriching(
        _ projection: WholeWorkflowResearchLibraryProjection
    ) -> Bool {
        !projection.isOfflineVariant
            && !context.researchRemoteSearchResultsAvailable
            && !context.searchQuery.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private func researchBadges(
        for result: WholeWorkflowResearchLibraryResult,
        in projection: WholeWorkflowResearchLibraryProjection
    ) -> [String] {
        var badges = [projection.resultState(for: result).localizedLabel]
        if result.recommended {
            badges.append(L10n.App.Packages.Research.recommended.localized)
        }
        return badges
    }

    private var productionLibraryTableRows: [LibraryTableRow] {
        let activeManagerFilterId = selectedManagerId ?? context.managerFilterId
        return displayedPackages.map { packageRow in
            let preferredManagerId = activeManagerFilterId
                ?? core.preferredManagerId(for: packageRow.package)
            let exactSelectedPackageId = packageRow.containsPackageId(context.selectedPackageId)
                ? context.selectedPackageId
                : nil
            let package = packageRow.actionTarget(
                preferredManagerId: preferredManagerId,
                selectedPackageId: exactSelectedPackageId
            )
            let action = primaryPackageAction(
                for: packageRow,
                actionTarget: package,
                managerConstraint: activeManagerFilterId
            )
            return LibraryTableRow(
                id: packageRow.id,
                representedPackageIDs: packageRow.memberPackages.map(\.id),
                selectedPackageID: package.id,
                selectedManagerID: package.managerId,
                name: package.displayName,
                detail: libraryTableDetail(
                    secondaryText: package.summary,
                    badges: rowDetailBadges(for: packageRow, actionTarget: package)
                ),
                manager: packageRow.managerDisplayText,
                currentVersion: package.version,
                latestVersion: package.latestVersion,
                status: package.status.displayName,
                statusSymbolName: package.status.iconName,
                statusTone: libraryTableStatusTone(for: package.status),
                isPinned: package.pinned,
                isRestartRequired: package.restartRequired,
                action: LibraryTableAction(
                    symbolName: action.symbol,
                    title: action.tooltip,
                    isEnabled: action.enabled,
                    isInFlight: action.inFlight
                )
            )
        }
    }

    private func researchLibraryTableRows(
        _ results: [WholeWorkflowResearchLibraryResult],
        in projection: WholeWorkflowResearchLibraryProjection
    ) -> [LibraryTableRow] {
        results.map { result in
            let package = result.packageItem
            let hasInstallReview = projection.installConfirmation(forPackageID: result.id) != nil
            return LibraryTableRow(
                id: result.id,
                representedPackageIDs: [result.id],
                selectedPackageID: result.id,
                selectedManagerID: result.managerID,
                name: package.displayName,
                detail: libraryTableDetail(
                    secondaryText: localizedResearchRecommendation(
                        key: result.recommendationReasonKey,
                        managerID: result.managerID
                    ),
                    badges: researchBadges(for: result, in: projection)
                ),
                manager: localizedManagerDisplayName(result.managerID),
                currentVersion: result.version,
                latestVersion: nil,
                status: package.status.displayName,
                statusSymbolName: package.status.iconName,
                statusTone: libraryTableStatusTone(for: package.status),
                isPinned: false,
                isRestartRequired: false,
                action: hasInstallReview
                    ? LibraryTableAction(
                        symbolName: "arrow.down.circle",
                        title: L10n.App.Packages.Research.reviewInstall.localized,
                        isEnabled: true,
                        isInFlight: false
                    )
                    : nil
            )
        }
    }

    private func libraryTable(rows: [LibraryTableRow]) -> some View {
        let selectedRowID = LibraryTableSelectionPolicy.selectedRowID(
            forPackageID: context.selectedPackageId,
            in: rows
        )
        let focusRequest = context.pendingLibraryPackageFocusRequest.flatMap { request in
            LibraryTableSelectionPolicy.selectedRowID(forPackageID: request.packageID, in: rows).map {
                LibraryTableFocusRequest(requestID: request.id, rowID: $0)
            }
        }
        return LibraryTableView(
            rows: rows,
            selectedRowID: selectedRowID,
            columnLabels: LibraryTableColumnLabels(
                package: L10n.App.Packages.Table.package.localized,
                manager: L10n.App.Inspector.manager.localized,
                version: L10n.App.Inspector.version.localized,
                status: L10n.App.Inspector.packageStatus.localized,
                currentVersion: L10n.App.Packages.Detail.Version.current.localized,
                latestVersion: L10n.App.Packages.Detail.Version.latest.localized,
                pinned: L10n.App.Packages.Label.pinned.localized,
                restartRequired: L10n.App.Packages.Label.restartRequired.localized,
                viewDetails: L10n.App.Packages.Action.viewDetails.localized
            ),
            accessibilityLabel: ControlCenterSection.packages.title,
            focusRequest: focusRequest,
            onSelectRow: selectLibraryTableRow,
            onShowDetails: showLibraryTableDetails,
            onPerformAction: performLibraryTableAction,
            onFulfillFocusRequest: completeLibraryTableFocusRequest
        )
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func libraryTableDetail(secondaryText: String?, badges: [String]) -> String? {
        var parts: [String] = []
        if let secondaryText {
            let trimmed = secondaryText.trimmingCharacters(in: .whitespacesAndNewlines)
            if !trimmed.isEmpty {
                parts.append(trimmed)
            }
        }
        parts.append(contentsOf: badges.filter { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty })
        return parts.isEmpty ? nil : parts.joined(separator: " • ")
    }

    private func libraryTableStatusTone(for status: PackageStatus) -> LibraryTableStatusTone {
        switch status {
        case .installed:
            return .healthy
        case .upgradable:
            return .updatesReady
        case .available:
            return .available
        }
    }

    private func selectLibraryTableRow(_ row: LibraryTableRow) {
        context.selectedPackageId = row.selectedPackageID
        context.selectedManagerId = row.selectedManagerID
        context.selectedTaskId = nil
        context.selectedUpgradePlanStepId = nil
    }

    private func showLibraryTableDetails(_ row: LibraryTableRow) {
        selectLibraryTableRow(row)
        context.isInspectorVisible = true
    }

    private func performLibraryTableAction(_ row: LibraryTableRow) {
        if let researchLibraryProjection {
            guard let confirmation = researchLibraryProjection.installConfirmation(
                forPackageID: row.selectedPackageID
            ) else {
                return
            }
            context.presentResearchInstallConfirmation(confirmation)
            return
        }

        guard let packageRow = displayedPackages.first(where: { $0.id == row.id }) else { return }
        let managerConstraint = selectedManagerId ?? context.managerFilterId
        let package = packageRow.actionTarget(
            preferredManagerId: managerConstraint ?? core.preferredManagerId(for: packageRow.package),
            selectedPackageId: row.selectedPackageID
        )
        primaryPackageAction(
            for: packageRow,
            actionTarget: package,
            managerConstraint: managerConstraint
        ).action?()
    }

    private func completeLibraryTableFocusRequest(_ requestID: Int) {
        guard let request = context.pendingLibraryPackageFocusRequest,
              request.id == requestID else {
            return
        }
        context.completeLibraryPackageFocusRequest(request, focusSucceeded: true)
    }

    private var focusablePackageRowIDs: [String] {
        if let researchLibraryProjection {
            return researchResults(in: researchLibraryProjection).map(\.id)
        }
        return displayedPackages.map(\.id)
    }

    private func preparePendingPackageFocusRequest() {
        guard context.pendingLibraryPackageFocusRequest != nil else { return }

        var filtersChanged = false
        if selectedStatusFilter != nil {
            selectedStatusFilter = nil
            filtersChanged = true
        }
        if showPinnedOnly {
            showPinnedOnly = false
            filtersChanged = true
        }
        if selectedManagerId != nil {
            selectedManagerId = nil
            filtersChanged = true
        }
        if context.managerFilterId != nil {
            context.managerFilterId = nil
            filtersChanged = true
        }
        if filtersChanged, researchLibraryProjection == nil {
            refreshPackageSnapshots()
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
        let isExternalSparkle = ExternalSparkleUpdatePolicy.primaryAction(
            forManagerId: package.managerId
        ) == .openApplication
        return PrimaryPackageAction(
            symbol: isExternalSparkle ? "arrow.up.forward.app" : "arrow.up.circle",
            tooltip: isExternalSparkle
                ? L10n.App.Updates.openAppToUpdate.localized
                : L10n.App.Packages.Action.upgradePackage.localized,
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
        if package.status == .available,
           let originLabel = package.resultProvenance?.origin.localizedLabel {
            badges.append(originLabel)
        }
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

extension WholeWorkflowResearchLibraryResult {
    var packageItem: PackageItem {
        PackageItem(
            id: id,
            name: packageName,
            version: version,
            managerId: managerID,
            manager: localizedManagerDisplayName(managerID),
            summary: localizedResearchRecommendation(
                key: recommendationReasonKey,
                managerID: managerID
            ),
            status: .available
        )
    }
}

extension WholeWorkflowResearchLibraryResultState {
    var localizedLabel: String {
        switch self {
        case .local:
            return L10n.App.Packages.Research.local.localized
        case .cached:
            return L10n.App.Packages.Research.cached.localized
        case .remote:
            return L10n.App.Packages.Research.remote.localized
        case .deferred:
            return L10n.App.Packages.Research.deferred.localized
        }
    }

    var symbolName: String {
        switch self {
        case .local:
            return "externaldrive.fill.badge.checkmark"
        case .cached:
            return "internaldrive.fill"
        case .remote:
            return "network"
        case .deferred:
            return "wifi.slash"
        }
    }

    var tintColor: Color {
        switch self {
        case .local:
            return HelmTheme.stateHealthy
        case .cached:
            return HelmTheme.actionSecondaryText
        case .remote:
            return HelmTheme.stateRunning
        case .deferred:
            return HelmTheme.stateUnavailable
        }
    }
}

func localizedResearchRecommendation(key: String, managerID: String) -> String {
    switch key {
    case "research.search.recommendation.existing_authority":
        return L10n.App.Packages.Research.existingAuthorityRecommendation.localized(
            with: ["manager": localizedManagerDisplayName(managerID)]
        )
    case "research.search.recommendation.alternate_source":
        return L10n.App.Packages.Research.alternateSourceRecommendation.localized
    default:
        return key.localized
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
