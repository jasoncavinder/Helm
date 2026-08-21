import Foundation

struct LibraryTableActionLabels {
    let unpin = L10n.App.Packages.Action.unpin.localized, install = L10n.App.Packages.Action.install.localized
    let openApplication = L10n.App.Updates.openAppToUpdate.localized
    let upgrade = L10n.App.Packages.Action.upgradePackage.localized
}

enum LibraryTableRowProjector {
    private struct Labels {
        let installed = PackageStatus.installed.displayName, upgradable = PackageStatus.upgradable.displayName
        let available = PackageStatus.available.displayName, active = L10n.App.Inspector.packageRuntimeStateActive.localized
        let defaultRuntime = L10n.App.Inspector.packageRuntimeStateDefault.localized, override = L10n.App.Inspector.packageRuntimeStateOverride.localized
    }

    static func project(
        revision: LibraryTableModelRevision,
        packageRows: [ConsolidatedPackageItem],
        selectedPackageID: String?,
        managerConstraint: String?,
        preferredManagerID: (PackageItem) -> String?,
        action: (ConsolidatedPackageItem, PackageItem) -> LibraryTableAction?
    ) -> LibraryTableSnapshot {
        let labels = Labels()
        var rows: [LibraryTableRow] = []
        rows.reserveCapacity(packageRows.count)
        var rowIndexesByID: [String: Int] = [:]
        rowIndexesByID.reserveCapacity(packageRows.count)
        var rowIDsByPackageID: [String: String] = [:]
        rowIDsByPackageID.reserveCapacity(packageRows.count)

        for packageRow in packageRows {
            let preferredManagerID = managerConstraint ?? preferredManagerID(packageRow.package)
            let exactSelectedPackageID = packageRow.containsPackageId(selectedPackageID) ? selectedPackageID : nil
            let package = packageRow.actionTarget(
                preferredManagerId: preferredManagerID,
                selectedPackageId: exactSelectedPackageID
            )
            let representedPackageIDs = packageRow.memberPackages.map(\.id)
            let rowIndex = rows.count
            rowIndexesByID[packageRow.id] = rowIndex
            for packageID in representedPackageIDs {
                rowIDsByPackageID[packageID] = packageRow.id
            }
            rows.append(
                LibraryTableRow(
                    id: packageRow.id,
                    representedPackageIDs: representedPackageIDs,
                    selectedPackageID: package.id,
                    selectedManagerID: package.managerId,
                    name: package.displayName,
                    detail: detail(secondaryText: package.summary, badges: detailBadges(
                        for: packageRow, actionTarget: package, labels: labels
                    )),
                    manager: packageRow.managerDisplayText,
                    currentVersion: PackageVersionPresentation.currentVersionText(
                        storedVersion: package.version,
                        localizedUnknown: L10n.Common.unknown.localized
                    ),
                    latestVersion: package.latestVersion,
                    status: statusLabel(for: package.status, labels: labels),
                    statusSymbolName: package.status.iconName,
                    statusTone: statusTone(for: package.status),
                    isPinned: package.pinned,
                    isRestartRequired: package.restartRequired,
                    action: action(packageRow, package)
                )
            )
        }

        return LibraryTableSnapshot(
            revision: revision, rows: rows,
            rowIndexesByID: rowIndexesByID, rowIDsByPackageID: rowIDsByPackageID
        )
    }

    static func sourceFingerprint(for packageRows: [ConsolidatedPackageItem]) -> Int {
        var hasher = Hasher()
        hasher.combine(packageRows.count)
        for packageRow in packageRows {
            hasher.combine(packageRow.id)
            hasher.combine(packageRow.managerIds)
            hasher.combine(packageRow.managerDisplayNames)
            combine(packageRow.package, into: &hasher)
            hasher.combine(packageRow.memberPackages.count)
            for package in packageRow.memberPackages {
                combine(package, into: &hasher)
            }
        }
        return hasher.finalize()
    }

    static func detail(secondaryText: String?, badges: [String]) -> String? {
        let secondaryText = secondaryText?.trimmingCharacters(in: .whitespacesAndNewlines)
        let badges = badges.filter { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
        guard !badges.isEmpty else {
            return secondaryText?.isEmpty == false ? secondaryText : nil
        }
        guard let secondaryText, !secondaryText.isEmpty else {
            return badges.joined(separator: " • ")
        }
        return ([secondaryText] + badges).joined(separator: " • ")
    }

    static func statusTone(for status: PackageStatus) -> LibraryTableStatusTone {
        switch status {
        case .installed: return .healthy
        case .upgradable: return .updatesReady
        case .available: return .available
        }
    }

    private static func detailBadges(
        for packageRow: ConsolidatedPackageItem,
        actionTarget package: PackageItem,
        labels: Labels
    ) -> [String] {
        let managerPackages = packageRow.packages(forManagerId: package.managerId)
        guard !managerPackages.isEmpty else { return [] }

        var badges: [String] = []
        if package.status == .available,
           let originLabel = package.resultProvenance?.origin.localizedLabel {
            badges.append(originLabel)
        }
        if managerPackages.count > 1 {
            let distinctVersions = Set(managerPackages.compactMap { candidate -> String? in
                let version = candidate.version.trimmingCharacters(in: .whitespacesAndNewlines)
                return PackageIdentity.hasKnownVersion(version) ? version : nil
            })
            let versionCount = distinctVersions.isEmpty ? managerPackages.count : distinctVersions.count
            if versionCount > 1 {
                badges.append(L10n.App.Packages.Label.versionCount.localized(with: [
                    "count": "\(versionCount)"
                ]))
            }
        }
        if managerPackages.contains(where: { $0.runtimeState.isActive }) {
            badges.append(labels.active)
        }
        if managerPackages.contains(where: { $0.runtimeState.isDefault }) {
            badges.append(labels.defaultRuntime)
        }
        if managerPackages.contains(where: { $0.runtimeState.hasOverride }) {
            badges.append(labels.override)
        }
        return badges
    }

    private static func statusLabel(for status: PackageStatus, labels: Labels) -> String {
        switch status {
        case .installed: return labels.installed
        case .upgradable: return labels.upgradable
        case .available: return labels.available
        }
    }

    private static func combine(_ package: PackageItem, into hasher: inout Hasher) {
        hasher.combine(package.id)
        hasher.combine(package.name)
        hasher.combine(package.packageIdentifier)
        hasher.combine(package.version)
        hasher.combine(package.latestVersion)
        hasher.combine(package.managerId)
        hasher.combine(package.manager)
        hasher.combine(package.summary)
        hasher.combine(package.pinned)
        hasher.combine(package.restartRequired)
        hasher.combine(package.runtimeState)
        hasher.combine(package.resultProvenance)
        hasher.combine(package.status.rawValue)
    }
}
