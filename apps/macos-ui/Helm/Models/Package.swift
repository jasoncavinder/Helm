import SwiftUI

struct PackageRuntimeState: Codable, Hashable {
    var isActive: Bool = false
    var isDefault: Bool = false
    var hasOverride: Bool = false

    var isEmpty: Bool {
        !isActive && !isDefault && !hasOverride
    }
}

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
        case .installed:  return HelmTheme.stateHealthy
        case .upgradable: return HelmTheme.stateUpdatesReady
        case .available:  return HelmTheme.actionSecondaryText
        }
    }
}

struct PackageItem: Identifiable {
    let id: String
    let name: String
    let packageIdentifier: String?
    let version: String
    var latestVersion: String?
    let managerId: String
    let manager: String
    var summary: String?
    var pinned: Bool = false
    var restartRequired: Bool = false
    var runtimeState: PackageRuntimeState = PackageRuntimeState()
    let resultProvenance: PackageResultProvenance?
    private var statusOverride: PackageStatus?

    var status: PackageStatus {
        if let override_ = statusOverride { return override_ }
        return latestVersion != nil ? .upgradable : .installed
    }

    init(id: String, name: String, packageIdentifier: String? = nil, version: String, latestVersion: String? = nil, managerId: String? = nil, manager: String, summary: String? = nil, pinned: Bool = false, restartRequired: Bool = false, runtimeState: PackageRuntimeState = PackageRuntimeState(), resultProvenance: PackageResultProvenance? = nil, status: PackageStatus? = nil) {
        self.id = id
        self.name = name
        if let packageIdentifier {
            let trimmedIdentifier = packageIdentifier.trimmingCharacters(in: .whitespacesAndNewlines)
            self.packageIdentifier = trimmedIdentifier.isEmpty ? nil : trimmedIdentifier
        } else {
            self.packageIdentifier = nil
        }
        self.version = version
        self.latestVersion = latestVersion
        self.managerId = managerId ?? manager
        self.manager = manager
        self.summary = summary
        self.pinned = pinned
        self.restartRequired = restartRequired
        self.runtimeState = runtimeState
        self.resultProvenance = resultProvenance
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

    var mutationPackageName: String {
        if managerId.lowercased() == "mas",
           let packageIdentifier,
           !packageIdentifier.isEmpty {
            return packageIdentifier
        }
        let trimmedName = name.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmedName.isEmpty ? name : trimmedName
    }

    var mutationTargetPackageName: String? {
        let displayName = name.trimmingCharacters(in: .whitespacesAndNewlines)
        let targetName = mutationPackageName.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !targetName.isEmpty else { return nil }
        return targetName == displayName ? nil : targetName
    }

    var mutationVersion: String? {
        let normalizedManagerId = managerId.lowercased()
        guard normalizedManagerId == "asdf" || normalizedManagerId == "mise",
              PackageIdentity.hasKnownVersion(version) else {
            return nil
        }
        let trimmedVersion = version.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmedVersion.isEmpty ? nil : trimmedVersion
    }
}

enum PackageVersionPresentation {
    static func currentVersionText(
        for package: PackageItem,
        localizedUnknown: String
    ) -> String {
        currentVersionText(
            storedVersion: package.version,
            localizedUnknown: localizedUnknown
        )
    }

    static func currentVersionText(
        storedVersion: String?,
        localizedUnknown: String
    ) -> String {
        PackageIdentity.normalizedKnownVersion(storedVersion) ?? localizedUnknown
    }
}

enum PackageVersionStorage {
    // Missing versions stay locale-neutral until a presentation surface renders them.
    static let unknown = ""
}

enum PackageMutationVersionPolicy {
    static func versionSelector(storedVersion: String?) -> String? {
        PackageIdentity.normalizedKnownVersion(storedVersion)
    }
}

enum PackageIdentity {
    // Matching-only values for every locale in LocalizationPreferenceStore.supportedSelections.
    // Keeping the bounded set here avoids capturing whichever locale happens to initialize the type.
    private static let unknownVersionTokens: Set<String> = [
        "unknown",
        "unbekannt",
        "desconocida",
        "inconnu",
        "ismeretlen",
        "不明",
        "desconhecido",
    ]

    static func normalizedBaseName(_ value: String) -> String {
        value.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
    }

    static func hasKnownVersion(_ value: String?) -> Bool {
        normalizedKnownVersion(value) != nil
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
        guard let normalizedVersion = normalizedKnownVersion(version) else { return nil }
        return qualifierFromSelector(normalizedVersion, lowercase: lowercase)
    }

    static func normalizedKnownVersion(_ value: String?) -> String? {
        guard let value else { return nil }
        let normalized = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !normalized.isEmpty else { return nil }
        if unknownVersionTokens.contains(where: {
            $0.caseInsensitiveCompare(normalized) == .orderedSame
        }) {
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

    func packages(forManagerId managerId: String) -> [PackageItem] {
        memberPackages.filter { $0.managerId == managerId }
    }

    func preferredPackage(forManagerId managerId: String, selectedPackageId: String? = nil) -> PackageItem? {
        let managerPackages = packages(forManagerId: managerId)
        guard !managerPackages.isEmpty else { return nil }
        if let selectedPackageId,
           let selectedPackage = managerPackages.first(where: { $0.id == selectedPackageId }) {
            return selectedPackage
        }
        return managerPackages.first
    }

    func actionTarget(preferredManagerId: String?, selectedPackageId: String? = nil) -> PackageItem {
        guard let targetIdentity = PackageMemberSelectionPolicy.actionTarget(
            members: memberPackages.map {
                PackageMemberIdentity(packageID: $0.id, managerID: $0.managerId)
            },
            orderedManagerIDs: managerIds,
            preferredManagerID: preferredManagerId,
            selectedPackageID: selectedPackageId
        ) else {
            return package
        }
        return memberPackages.first(where: { $0.id == targetIdentity.packageID }) ?? package
    }

    static func consolidate(
        _ packages: [PackageItem],
        locale: Locale = .current,
        localizedManagerName: (String) -> String,
        priorityRank: ((String) -> Int)? = nil
    ) -> [ConsolidatedPackageItem] {
        let grouped = Dictionary(grouping: packages) { $0.normalizedIdentityKey }

        let items = grouped.values.compactMap { members -> ConsolidatedPackageItem? in
            let sortedMembers = members.sorted { lhs, rhs in
                preferredPackageOrdering(
                    lhs,
                    rhs,
                    localizedManagerName: localizedManagerName,
                    priorityRank: priorityRank
                )
            }
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
                priorityRank: priorityRank
            )
            let managerDisplayNames = managerIds.map(localizedManagerName)

            return ConsolidatedPackageItem(
                package: primary,
                memberPackages: sortedMembers,
                managerIds: managerIds,
                managerDisplayNames: managerDisplayNames
            )
        }
        var indexedItems = items.map { item in
            (
                item: item,
                sortKey: item.package.displayName.folding(
                    options: [.caseInsensitive, .diacriticInsensitive],
                    locale: locale
                )
            )
        }
        indexedItems.sort { lhs, rhs in
            if lhs.sortKey != rhs.sortKey {
                return lhs.sortKey < rhs.sortKey
            }
            return isOrderedBefore(
                lhs.item,
                rhs.item,
                locale: locale,
                localizedManagerName: localizedManagerName,
                priorityRank: priorityRank
            )
        }

        let hasLocaleInversion = zip(indexedItems, indexedItems.dropFirst()).contains { pair in
            isOrderedBefore(
                pair.1.item,
                pair.0.item,
                locale: locale,
                localizedManagerName: localizedManagerName,
                priorityRank: priorityRank
            )
        }
        if hasLocaleInversion {
            indexedItems.sort { lhs, rhs in
                isOrderedBefore(
                    lhs.item,
                    rhs.item,
                    locale: locale,
                    localizedManagerName: localizedManagerName,
                    priorityRank: priorityRank
                )
            }
        }
        return indexedItems.map(\.item)
    }

    static func isOrderedBefore(
        _ lhs: ConsolidatedPackageItem,
        _ rhs: ConsolidatedPackageItem,
        locale: Locale,
        localizedManagerName: (String) -> String,
        priorityRank: ((String) -> Int)? = nil
    ) -> Bool {
        let nameOrder = lhs.package.displayName.compare(
            rhs.package.displayName,
            options: [.caseInsensitive],
            range: nil,
            locale: locale
        )
        if nameOrder != .orderedSame {
            return nameOrder == .orderedAscending
        }
        if preferredPackageOrdering(
            lhs.package,
            rhs.package,
            localizedManagerName: localizedManagerName,
            priorityRank: priorityRank
        ) {
            return true
        }
        if preferredPackageOrdering(
            rhs.package,
            lhs.package,
            localizedManagerName: localizedManagerName,
            priorityRank: priorityRank
        ) {
            return false
        }
        if lhs.package.normalizedIdentityKey != rhs.package.normalizedIdentityKey {
            return lhs.package.normalizedIdentityKey < rhs.package.normalizedIdentityKey
        }
        return lhs.package.id < rhs.package.id
    }

    private static func preferredPackageOrdering(
        _ lhs: PackageItem,
        _ rhs: PackageItem,
        localizedManagerName: (String) -> String,
        priorityRank: ((String) -> Int)?
    ) -> Bool {
        let lhsPreferred = PackageConsolidationPolicy.shouldPrefer(
            lhsStatus: lhs.status.rawValue,
            rhsStatus: rhs.status.rawValue,
            lhsPinned: lhs.pinned,
            rhsPinned: rhs.pinned,
            lhsRestartRequired: lhs.restartRequired,
            rhsRestartRequired: rhs.restartRequired,
            lhsRuntimeState: PackageRuntimeStateProjection(
                isActive: lhs.runtimeState.isActive,
                isDefault: lhs.runtimeState.isDefault,
                hasOverride: lhs.runtimeState.hasOverride
            ),
            rhsRuntimeState: PackageRuntimeStateProjection(
                isActive: rhs.runtimeState.isActive,
                isDefault: rhs.runtimeState.isDefault,
                hasOverride: rhs.runtimeState.hasOverride
            ),
            lhsVersion: lhs.version,
            rhsVersion: rhs.version,
            lhsManagerId: lhs.managerId,
            rhsManagerId: rhs.managerId,
            localizedManagerName: localizedManagerName,
            priorityRank: priorityRank
        )
        let rhsPreferred = PackageConsolidationPolicy.shouldPrefer(
            lhsStatus: rhs.status.rawValue,
            rhsStatus: lhs.status.rawValue,
            lhsPinned: rhs.pinned,
            rhsPinned: lhs.pinned,
            lhsRestartRequired: rhs.restartRequired,
            rhsRestartRequired: lhs.restartRequired,
            lhsRuntimeState: PackageRuntimeStateProjection(
                isActive: rhs.runtimeState.isActive,
                isDefault: rhs.runtimeState.isDefault,
                hasOverride: rhs.runtimeState.hasOverride
            ),
            rhsRuntimeState: PackageRuntimeStateProjection(
                isActive: lhs.runtimeState.isActive,
                isDefault: lhs.runtimeState.isDefault,
                hasOverride: lhs.runtimeState.hasOverride
            ),
            lhsVersion: rhs.version,
            rhsVersion: lhs.version,
            lhsManagerId: rhs.managerId,
            rhsManagerId: lhs.managerId,
            localizedManagerName: localizedManagerName,
            priorityRank: priorityRank
        )
        if lhsPreferred != rhsPreferred {
            return lhsPreferred
        }
        if lhs.id != rhs.id {
            return lhs.id < rhs.id
        }
        return lhs.managerId < rhs.managerId
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
