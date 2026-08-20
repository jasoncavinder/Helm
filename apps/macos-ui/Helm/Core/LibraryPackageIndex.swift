import Foundation

struct LibraryPackageIndex {
    private struct Member {
        let packageIndex: Int
        let facts: LibraryPackageIndexMemberFacts
    }

    private struct Group {
        let projection: ConsolidatedPackageItem
        let members: [Member]
        let normalizedIdentityKey: String
        let normalizedBaseName: String
    }

    let localeIdentifier: String
    private let groups: [Group]

    init(
        packages: [PackageItem],
        localeIdentifier: String,
        localizedManagerName: (String) -> String
    ) {
        self.localeIdentifier = localeIdentifier
        groups = ConsolidatedPackageItem.consolidate(
            packages,
            localizedManagerName: localizedManagerName
        ).map { projection in
            Group(
                projection: projection,
                members: projection.memberPackages.enumerated().map { index, package in
                    Member(
                        packageIndex: index,
                        facts: LibraryPackageIndexMemberFacts(
                            managerID: package.managerId,
                            status: package.status.rawValue,
                            isPinned: package.pinned,
                            normalizedIdentityKey: package.normalizedIdentityKey,
                            managerSearchText: package.manager.lowercased(),
                            summarySearchText: package.summary?.lowercased() ?? ""
                        )
                    )
                },
                normalizedIdentityKey: projection.package.normalizedIdentityKey,
                normalizedBaseName: projection.package.normalizedBaseName
            )
        }
    }

    func filteredPackages(
        query: String,
        managerID: String?,
        statusFilter: PackageStatus?,
        pinnedOnly: Bool,
        managerParticipatesInSearch: (String) -> Bool,
        localizedManagerName: (String) -> String
    ) -> [ConsolidatedPackageItem] {
        let queryToken = query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        let normalizedQueryToken = PackageIdentity.normalizedExactQueryToken(queryToken)
        let queryBaseToken = PackageIdentity.normalizedQueryBaseToken(queryToken)
        let queryHasQualifier = normalizedQueryToken.contains("@")

        var qualifiedMatches: [ConsolidatedPackageItem] = []
        var baseMatches: [ConsolidatedPackageItem] = []
        var remainingPackages: [ConsolidatedPackageItem] = []
        qualifiedMatches.reserveCapacity(1)
        baseMatches.reserveCapacity(1)
        remainingPackages.reserveCapacity(groups.count)

        for group in groups {
            let projection: ConsolidatedPackageItem?
            if group.members.count == 1, let member = group.members.first {
                projection = matches(
                    member,
                    queryToken: queryToken,
                    normalizedQueryToken: normalizedQueryToken,
                    managerID: managerID,
                    statusFilter: statusFilter,
                    pinnedOnly: pinnedOnly,
                    managerParticipatesInSearch: managerParticipatesInSearch
                ) ? group.projection : nil
            } else {
                let matchingPackages = group.members.compactMap { member -> PackageItem? in
                    guard matches(
                        member,
                        queryToken: queryToken,
                        normalizedQueryToken: normalizedQueryToken,
                        managerID: managerID,
                        statusFilter: statusFilter,
                        pinnedOnly: pinnedOnly,
                        managerParticipatesInSearch: managerParticipatesInSearch
                    ) else {
                        return nil
                    }
                    return group.projection.memberPackages[member.packageIndex]
                }
                if matchingPackages.isEmpty {
                    projection = nil
                } else if matchingPackages.count == group.members.count {
                    projection = group.projection
                } else {
                    projection = ConsolidatedPackageItem.consolidate(
                        matchingPackages,
                        localizedManagerName: localizedManagerName
                    ).first
                }
            }

            guard let projection else { continue }
            if queryBaseToken.isEmpty {
                remainingPackages.append(projection)
            } else if queryHasQualifier,
                      group.normalizedIdentityKey == normalizedQueryToken {
                qualifiedMatches.append(projection)
            } else if group.normalizedBaseName == queryBaseToken {
                baseMatches.append(projection)
            } else {
                remainingPackages.append(projection)
            }
        }

        return qualifiedMatches + baseMatches + remainingPackages
    }

    private func matches(
        _ member: Member,
        queryToken: String,
        normalizedQueryToken: String,
        managerID: String?,
        statusFilter: PackageStatus?,
        pinnedOnly: Bool,
        managerParticipatesInSearch: (String) -> Bool
    ) -> Bool {
        LibraryPackageIndexMatchPolicy.matches(
            member.facts,
            queryToken: queryToken,
            normalizedQueryToken: normalizedQueryToken,
            managerID: managerID,
            statusFilter: statusFilter?.rawValue,
            pinnedOnly: pinnedOnly,
            managerParticipatesInSearch: managerParticipatesInSearch
        )
    }
}
