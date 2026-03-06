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
    var latestVersion: String?
    let managerId: String
    let manager: String
    var summary: String?
    var pinned: Bool = false
    var restartRequired: Bool = false
    private var statusOverride: PackageStatus?

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

    var displayName: String {
        PackageIdentity.displayName(name: name, version: version)
    }

    var normalizedIdentityKey: String {
        PackageIdentity.normalizedIdentityKey(name: name, version: version)
    }

    var normalizedBaseName: String {
        PackageIdentity.normalizedBaseName(name)
    }
}

enum PackageIdentity {
    private static let unknownVersionTokens: Set<String> = {
        var tokens: Set<String> = ["unknown"]
        let localizedUnknown = L10n.Common.unknown.localized
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()
        if !localizedUnknown.isEmpty {
            tokens.insert(localizedUnknown)
        }
        return tokens
    }()

    static func normalizedBaseName(_ value: String) -> String {
        value.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
    }

    static func variantQualifier(fromVersion version: String?) -> String? {
        normalizedVariantQualifier(fromVersion: version, lowercase: false)
    }

    static func normalizedVariantQualifier(fromVersion version: String?) -> String? {
        normalizedVariantQualifier(fromVersion: version, lowercase: true)
    }

    static func displayName(name: String, version: String?) -> String {
        let trimmedName = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedName.isEmpty else { return name }
        guard let qualifier = variantQualifier(fromVersion: version) else { return trimmedName }
        return "\(trimmedName)@\(qualifier)"
    }

    static func normalizedIdentityKey(name: String, version: String?) -> String {
        let normalizedName = normalizedBaseName(name)
        guard !normalizedName.isEmpty else { return "" }
        guard let qualifier = normalizedVariantQualifier(fromVersion: version) else {
            return normalizedName
        }
        return "\(normalizedName)@\(qualifier)"
    }

    static func normalizedExactQueryToken(_ value: String) -> String {
        let normalized = value
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()
        guard !normalized.isEmpty else { return "" }
        guard let atIndex = normalized.lastIndex(of: "@"),
              atIndex != normalized.startIndex else {
            return normalized
        }
        let base = String(normalized[..<atIndex]).trimmingCharacters(in: .whitespacesAndNewlines)
        guard !base.isEmpty else { return normalized }
        let selector = String(normalized[normalized.index(after: atIndex)...])
        guard !selector.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            return String(base)
        }
        if let qualifier = qualifierFromSelector(selector, lowercase: true) {
            return "\(base)@\(qualifier)"
        }
        return String(base)
    }

    static func normalizedQueryBaseToken(_ value: String) -> String {
        let normalized = value
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()
        guard !normalized.isEmpty else { return "" }
        guard let atIndex = normalized.lastIndex(of: "@"),
              atIndex != normalized.startIndex else {
            return normalized
        }
        let base = String(normalized[..<atIndex]).trimmingCharacters(in: .whitespacesAndNewlines)
        return base.isEmpty ? normalized : String(base)
    }

    private static func normalizedVariantQualifier(fromVersion version: String?, lowercase: Bool) -> String? {
        guard let normalizedVersion = normalizedVersionSelectorInput(version) else { return nil }
        return qualifierFromSelector(normalizedVersion, lowercase: lowercase)
    }

    private static func normalizedVersionSelectorInput(_ value: String?) -> String? {
        guard let value else { return nil }
        let normalized = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !normalized.isEmpty else { return nil }
        if unknownVersionTokens.contains(normalized.lowercased()) {
            return nil
        }
        return normalized
    }

    private static func qualifierFromSelector(_ selector: String, lowercase: Bool) -> String? {
        let atoms = selector
            .split(separator: "-")
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
        guard !atoms.isEmpty else { return nil }

        let firstReleaseAtom = atoms.firstIndex(where: { atom in
            isReleaseTokenAtom(atom)
        })
        let qualifierAtoms: ArraySlice<String> = {
            guard let firstReleaseAtom else {
                return atoms[atoms.startIndex..<atoms.endIndex]
            }
            guard firstReleaseAtom > 0 else { return [] }
            return atoms[atoms.startIndex..<firstReleaseAtom]
        }()
        guard !qualifierAtoms.isEmpty else { return nil }
        let qualifier = qualifierAtoms.joined(separator: "-")
        return lowercase ? qualifier.lowercased() : qualifier
    }

    private static func isReleaseTokenAtom(_ atom: String) -> Bool {
        guard let first = atom.first else { return false }
        if first.isNumber {
            return true
        }
        if first == "v" || first == "V" {
            let next = atom.dropFirst().first
            return next?.isNumber == true
        }
        return false
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

    func actionTarget(preferredManagerId: String?) -> PackageItem {
        guard let managerId = PackageConsolidationPolicy.preferredManagerId(
            managerIds: managerIds,
            preferredManagerId: preferredManagerId
        ) else {
            return package
        }
        return memberPackages.first(where: { $0.managerId == managerId }) ?? package
    }

    static func consolidate(
        _ packages: [PackageItem],
        localizedManagerName: (String) -> String
    ) -> [ConsolidatedPackageItem] {
        let grouped = Dictionary(grouping: packages) { $0.normalizedIdentityKey }

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
            let nameOrder = lhs.package.displayName.localizedCaseInsensitiveCompare(rhs.package.displayName)
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
            lhsVersion: lhs.version,
            rhsVersion: rhs.version,
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
