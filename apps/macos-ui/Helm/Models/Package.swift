import SwiftUI

enum PackageStatus: String, CaseIterable {
    case installed
    case upgradable
    case available

    var displayName: String {
        switch self {
        case .installed:  return L10n.App.Packages.Filter.installed.localized
        case .upgradable: return L10n.App.Packages.Filter.upgradable.localized
        case .available:  return L10n.App.Packages.Filter.available.localized
        }
    }

    var iconName: String {
        switch self {
        case .installed:  return "checkmark.circle.fill"
        case .upgradable: return "arrow.up.circle.fill"
        case .available:  return "plus.circle.fill"
        }
    }

    var iconColor: Color {
        switch self {
        case .installed:  return .green
        case .upgradable: return .orange
        case .available:  return .blue
        }
    }
}

struct PackageItem: Identifiable {
    let id: String
    let name: String
    let version: String
    var latestVersion: String? = nil
    let managerId: String
    let manager: String
    var summary: String? = nil
    var pinned: Bool = false
    var restartRequired: Bool = false
    private var statusOverride: PackageStatus? = nil

    var status: PackageStatus {
        if let override_ = statusOverride { return override_ }
        return latestVersion != nil ? .upgradable : .installed
    }

    init(id: String, name: String, version: String, latestVersion: String? = nil, managerId: String? = nil, manager: String, summary: String? = nil, pinned: Bool = false, restartRequired: Bool = false, status: PackageStatus? = nil) {
        self.id = id
        self.name = name
        self.version = version
        self.latestVersion = latestVersion
        self.managerId = managerId ?? manager
        self.manager = manager
        self.summary = summary
        self.pinned = pinned
        self.restartRequired = restartRequired
        self.statusOverride = status
    }
}

struct ConsolidatedPackageItem: Identifiable {
    let package: PackageItem
    let memberPackages: [PackageItem]
    let managerIds: [String]
    let managerDisplayNames: [String]

    var id: String { package.id }

    var managerDisplayText: String {
        managerDisplayNames.joined(separator: ", ")
    }

    func containsPackageId(_ packageId: String?) -> Bool {
        guard let packageId else { return false }
        return memberPackages.contains { $0.id == packageId }
    }

    static func consolidate(
        _ packages: [PackageItem],
        localizedManagerName: (String) -> String
    ) -> [ConsolidatedPackageItem] {
        let grouped = Dictionary(grouping: packages) { $0.name.lowercased() }

        return grouped.values.compactMap { members in
            let sortedMembers = members.sorted(by: preferredPackageOrdering)
            guard var primary = sortedMembers.first else { return nil }

            for member in sortedMembers.dropFirst() {
                mergeSummary(into: &primary, from: member.summary)
                if primary.latestVersion == nil {
                    primary.latestVersion = member.latestVersion
                }
                primary.restartRequired = primary.restartRequired || member.restartRequired
            }

            let managerIds = PackageConsolidationPolicy.sortedManagerIds(
                sortedMembers.map(\.managerId),
                localizedManagerName: localizedManagerName,
                priorityRank: { HelmCore.shared.managerPriorityRank(for: $0) }
            )
            let managerDisplayNames = managerIds.map(localizedManagerName)

            return ConsolidatedPackageItem(
                package: primary,
                memberPackages: sortedMembers,
                managerIds: managerIds,
                managerDisplayNames: managerDisplayNames
            )
        }
        .sorted { lhs, rhs in
            let nameOrder = lhs.package.name.localizedCaseInsensitiveCompare(rhs.package.name)
            if nameOrder != .orderedSame {
                return nameOrder == .orderedAscending
            }
            return preferredPackageOrdering(lhs.package, rhs.package)
        }
    }

    private static func preferredPackageOrdering(_ lhs: PackageItem, _ rhs: PackageItem) -> Bool {
        PackageConsolidationPolicy.shouldPrefer(
            lhsStatus: lhs.status.rawValue,
            rhsStatus: rhs.status.rawValue,
            lhsPinned: lhs.pinned,
            rhsPinned: rhs.pinned,
            lhsRestartRequired: lhs.restartRequired,
            rhsRestartRequired: rhs.restartRequired,
            lhsManagerId: lhs.managerId,
            rhsManagerId: rhs.managerId,
            localizedManagerName: localizedManagerDisplayName,
            priorityRank: { HelmCore.shared.managerPriorityRank(for: $0) }
        )
    }

    private static func mergeSummary(into package: inout PackageItem, from candidate: String?) {
        let existingSummary = package.summary?.trimmingCharacters(in: .whitespacesAndNewlines)
        guard existingSummary?.isEmpty != false else { return }
        guard let candidate else { return }
        let trimmedCandidate = candidate.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedCandidate.isEmpty else { return }
        package.summary = trimmedCandidate
    }
}
