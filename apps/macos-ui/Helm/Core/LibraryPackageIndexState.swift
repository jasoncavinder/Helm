import Foundation

enum PackageSnapshotPublicationPolicy {
    private struct SnapshotKey: Hashable {
        let id: String
        let name: String
        let packageIdentifier: String?
        let version: String
        let latestVersion: String?
        let managerID: String
        let manager: String
        let summary: String?
        let pinned: Bool
        let restartRequired: Bool
        let runtimeState: PackageRuntimeState
        let resultProvenance: PackageResultProvenance?
        let status: String

        init(_ package: PackageItem) {
            id = package.id
            name = package.name
            packageIdentifier = package.packageIdentifier
            version = PackageIdentity.normalizedKnownVersion(package.version)
                ?? PackageVersionStorage.unknown
            latestVersion = package.latestVersion
            managerID = package.managerId
            manager = package.manager
            summary = package.summary
            pinned = package.pinned
            restartRequired = package.restartRequired
            runtimeState = package.runtimeState
            resultProvenance = package.resultProvenance
            status = package.status.rawValue
        }
    }

    static func shouldPublish(
        current: [PackageItem],
        replacement: [PackageItem]
    ) -> Bool {
        !areSemanticallyEquivalent(current, replacement)
    }

    static func areSemanticallyEquivalent(
        _ lhs: [PackageItem],
        _ rhs: [PackageItem]
    ) -> Bool {
        guard lhs.count == rhs.count else { return false }
        var counts: [SnapshotKey: Int] = [:]
        counts.reserveCapacity(lhs.count)
        for package in lhs {
            counts[SnapshotKey(package), default: 0] += 1
        }
        for package in rhs {
            let key = SnapshotKey(package)
            guard let count = counts[key] else { return false }
            if count == 1 {
                counts.removeValue(forKey: key)
            } else {
                counts[key] = count - 1
            }
        }
        return counts.isEmpty
    }

    static func areOrderedSemanticallyEquivalent(
        _ lhs: [PackageItem],
        _ rhs: [PackageItem]
    ) -> Bool {
        guard lhs.count == rhs.count else { return false }
        return zip(lhs, rhs).allSatisfy { SnapshotKey($0) == SnapshotKey($1) }
    }
}

enum LibraryPackageIndexInvalidationPolicy {
    static func managerEnablementChanged(
        previous: [String: Bool],
        current: [String: Bool]
    ) -> Bool {
        disabledManagers(in: previous) != disabledManagers(in: current)
    }

    private static func disabledManagers(in statuses: [String: Bool]) -> Set<String> {
        Set(statuses.compactMap { managerID, enabled in enabled ? nil : managerID })
    }
}

struct LibraryPackageIndexCache {
    private(set) var cachedIndex: LibraryPackageIndex?
    private(set) var generation: UInt64 = 0
    private var sourceRevision: UInt64?

    mutating func resolve(
        packages: [PackageItem],
        sourceRevision: UInt64,
        localeIdentifier: String,
        localizedManagerName: (String) -> String,
        priorityRank: @escaping (String) -> Int
    ) -> LibraryPackageIndex {
        if let cachedIndex,
           self.sourceRevision == sourceRevision,
           cachedIndex.localeIdentifier == localeIdentifier {
            return cachedIndex
        }

        let index = LibraryPackageIndex(
            packages: packages,
            localeIdentifier: localeIdentifier,
            localizedManagerName: localizedManagerName,
            priorityRank: priorityRank
        )
        store(index, sourceRevision: sourceRevision)
        return index
    }

    mutating func store(_ index: LibraryPackageIndex, sourceRevision: UInt64) {
        cachedIndex = index
        self.sourceRevision = sourceRevision
        generation &+= 1
    }

    mutating func replace(_ index: LibraryPackageIndex?, sourceRevision: UInt64) {
        if let index {
            store(index, sourceRevision: sourceRevision)
        } else {
            invalidate()
        }
    }

    mutating func invalidate() {
        cachedIndex = nil
        sourceRevision = nil
    }
}

struct LibraryPackageSearchOverlayCache {
    private(set) var generation: UInt64 = 0
    private var cachedOverlay: LibraryPackageSearchOverlay?
    private var sourceRevision: UInt64?

    mutating func resolve(
        packages: [PackageItem],
        sourceRevision: UInt64
    ) -> LibraryPackageSearchOverlay {
        if let cachedOverlay, self.sourceRevision == sourceRevision {
            return cachedOverlay
        }
        let overlay = LibraryPackageSearchOverlay(packages: packages)
        cachedOverlay = overlay
        self.sourceRevision = sourceRevision
        generation &+= 1
        return overlay
    }

    mutating func invalidate() {
        cachedOverlay = nil
        sourceRevision = nil
    }
}

struct LibraryPackageSearchOverlay {
    private struct Member {
        let package: PackageItem
        let facts: LibraryPackageIndexMemberFacts
    }

    private let members: [Member]

    var isEmpty: Bool { members.isEmpty }

    init(packages: [PackageItem]) {
        members = packages.map { package in
            Member(
                package: package,
                facts: LibraryPackageIndex.facts(for: package)
            )
        }
    }

    func matchingPackagesByID(
        queryToken: String,
        normalizedQueryToken: String,
        managerParticipatesInSearch: (String) -> Bool,
        managerIsEnabled: (String) -> Bool
    ) -> [String: PackageItem] {
        var matchingPackages: [String: PackageItem] = [:]
        matchingPackages.reserveCapacity(members.count)
        for member in members {
            guard managerIsEnabled(member.facts.managerID),
                  LibraryPackageIndexMatchPolicy.matches(
                    member.facts,
                    queryToken: queryToken,
                    normalizedQueryToken: normalizedQueryToken,
                    managerID: nil,
                    statusFilter: nil,
                    pinnedOnly: false,
                    managerParticipatesInSearch: managerParticipatesInSearch
                  ) else {
                continue
            }
            if var existing = matchingPackages[member.package.id] {
                mergeSearchMetadata(into: &existing, from: member.package)
                matchingPackages[member.package.id] = existing
            } else {
                matchingPackages[member.package.id] = member.package
            }
        }
        return matchingPackages
    }
}

@discardableResult
func mergeSearchMetadata(into package: inout PackageItem, from candidate: PackageItem) -> Bool {
    var changed = false
    let existingSummary = package.summary?.trimmingCharacters(in: .whitespacesAndNewlines)
    if existingSummary?.isEmpty != false,
       let candidateSummary = candidate.summary?.trimmingCharacters(in: .whitespacesAndNewlines),
       !candidateSummary.isEmpty {
        package.summary = candidateSummary
        changed = true
    }
    if package.latestVersion == nil, candidate.latestVersion != nil {
        package.latestVersion = candidate.latestVersion
        changed = true
    }
    return changed
}
