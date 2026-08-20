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
    }

    let localeIdentifier: String
    private let locale: Locale
    private let groups: [Group]

    init(
        packages: [PackageItem],
        localeIdentifier: String,
        localizedManagerName: (String) -> String,
        priorityRank: ((String) -> Int)? = nil
    ) {
        self.localeIdentifier = localeIdentifier
        let resolvedLocale = Locale(identifier: localeIdentifier)
        locale = resolvedLocale
        groups = ConsolidatedPackageItem.consolidate(
            packages,
            locale: resolvedLocale,
            localizedManagerName: localizedManagerName,
            priorityRank: priorityRank
        ).map { projection in
            Group(
                projection: projection,
                members: projection.memberPackages.enumerated().map { index, package in
                    Member(
                        packageIndex: index,
                        facts: Self.facts(for: package)
                    )
                },
                normalizedIdentityKey: projection.package.normalizedIdentityKey
            )
        }
    }

    func filteredPackages(
        query: String,
        managerID: String?,
        statusFilter: PackageStatus?,
        pinnedOnly: Bool,
        overlay: LibraryPackageSearchOverlay? = nil,
        managerParticipatesInSearch: (String) -> Bool,
        managerIsEnabled: (String) -> Bool = { _ in true },
        localizedManagerName: (String) -> String,
        priorityRank: ((String) -> Int)? = nil
    ) -> [ConsolidatedPackageItem] {
        let queryToken = query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        let normalizedQueryToken = PackageIdentity.normalizedExactQueryToken(queryToken)
        let queryBaseToken = PackageIdentity.normalizedQueryBaseToken(queryToken)
        let queryHasQualifier = normalizedQueryToken.contains("@")

        let sortedPackages: [ConsolidatedPackageItem]
        if let overlay, !overlay.isEmpty, !queryToken.isEmpty {
            sortedPackages = filteredPackagesWithOverlay(
                overlay,
                queryToken: queryToken,
                normalizedQueryToken: normalizedQueryToken,
                managerID: managerID,
                statusFilter: statusFilter,
                pinnedOnly: pinnedOnly,
                managerParticipatesInSearch: managerParticipatesInSearch,
                managerIsEnabled: managerIsEnabled,
                localizedManagerName: localizedManagerName,
                priorityRank: priorityRank
            )
        } else {
            sortedPackages = filteredBasePackages(
                queryToken: queryToken,
                normalizedQueryToken: normalizedQueryToken,
                managerID: managerID,
                statusFilter: statusFilter,
                pinnedOnly: pinnedOnly,
                managerParticipatesInSearch: managerParticipatesInSearch,
                localizedManagerName: localizedManagerName,
                priorityRank: priorityRank
            )
        }

        return prioritizeExactMatches(
            sortedPackages,
            queryBaseToken: queryBaseToken,
            normalizedQueryToken: normalizedQueryToken,
            queryHasQualifier: queryHasQualifier
        )
    }

    private func filteredBasePackages(
        queryToken: String,
        normalizedQueryToken: String,
        managerID: String?,
        statusFilter: PackageStatus?,
        pinnedOnly: Bool,
        managerParticipatesInSearch: (String) -> Bool,
        localizedManagerName: (String) -> String,
        priorityRank: ((String) -> Int)?
    ) -> [ConsolidatedPackageItem] {
        var packages: [ConsolidatedPackageItem] = []
        packages.reserveCapacity(groups.count)
        for group in groups {
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
            guard !matchingPackages.isEmpty else { continue }
            if matchingPackages.count == group.members.count {
                packages.append(group.projection)
            } else if let projection = consolidate(
                matchingPackages,
                localizedManagerName: localizedManagerName,
                priorityRank: priorityRank
            ) {
                packages.append(projection)
            }
        }
        return packages
    }

    private func filteredPackagesWithOverlay(
        _ overlay: LibraryPackageSearchOverlay,
        queryToken: String,
        normalizedQueryToken: String,
        managerID: String?,
        statusFilter: PackageStatus?,
        pinnedOnly: Bool,
        managerParticipatesInSearch: (String) -> Bool,
        managerIsEnabled: (String) -> Bool,
        localizedManagerName: (String) -> String,
        priorityRank: ((String) -> Int)?
    ) -> [ConsolidatedPackageItem] {
        var remainingRemoteByID = overlay.matchingPackagesByID(
            queryToken: queryToken,
            normalizedQueryToken: normalizedQueryToken,
            managerParticipatesInSearch: managerParticipatesInSearch,
            managerIsEnabled: managerIsEnabled
        )

        var baseProjections: [ConsolidatedPackageItem] = []
        baseProjections.reserveCapacity(groups.count)
        for group in groups {
            var matchingPackages: [PackageItem] = []
            matchingPackages.reserveCapacity(group.members.count)
            var canReuseProjection = true

            for member in group.members {
                guard matchesQuery(
                    member,
                    queryToken: queryToken,
                    normalizedQueryToken: normalizedQueryToken,
                    managerParticipatesInSearch: managerParticipatesInSearch
                ) else {
                    canReuseProjection = false
                    continue
                }

                var package = group.projection.memberPackages[member.packageIndex]
                if let remote = remainingRemoteByID.removeValue(forKey: package.id) {
                    if mergeSearchMetadata(into: &package, from: remote) {
                        canReuseProjection = false
                    }
                }
                guard matchesFilters(
                    package,
                    managerID: managerID,
                    statusFilter: statusFilter,
                    pinnedOnly: pinnedOnly
                ) else {
                    canReuseProjection = false
                    continue
                }
                matchingPackages.append(package)
            }

            guard !matchingPackages.isEmpty else { continue }
            if canReuseProjection, matchingPackages.count == group.members.count {
                baseProjections.append(group.projection)
            } else if let projection = consolidate(
                matchingPackages,
                localizedManagerName: localizedManagerName,
                priorityRank: priorityRank
            ) {
                baseProjections.append(projection)
            }
        }

        let remainingRemoteByIdentity = Dictionary(
            grouping: remainingRemoteByID.values.filter { package in
                matchesFilters(
                    package,
                    managerID: managerID,
                    statusFilter: statusFilter,
                    pinnedOnly: pinnedOnly
                )
            },
            by: \.normalizedIdentityKey
        )
        guard !remainingRemoteByIdentity.isEmpty else { return baseProjections }
        let remainingIdentities = Set(remainingRemoteByIdentity.keys)
        var baseProjectionIndexByIdentity: [String: Int] = [:]
        baseProjectionIndexByIdentity.reserveCapacity(remainingIdentities.count)
        for (index, projection) in baseProjections.enumerated()
        where remainingIdentities.contains(projection.package.normalizedIdentityKey) {
            baseProjectionIndexByIdentity[projection.package.normalizedIdentityKey] = index
        }
        var remoteOnlyPackages: [PackageItem] = []
        remoteOnlyPackages.reserveCapacity(remainingRemoteByID.count)
        for (identity, remotePackages) in remainingRemoteByIdentity {
            guard let index = baseProjectionIndexByIdentity[identity] else {
                remoteOnlyPackages.append(contentsOf: remotePackages)
                continue
            }
            if let projection = consolidate(
                baseProjections[index].memberPackages + remotePackages,
                localizedManagerName: localizedManagerName,
                priorityRank: priorityRank
            ) {
                baseProjections[index] = projection
            }
        }
        let remoteOnlyProjections = ConsolidatedPackageItem.consolidate(
            remoteOnlyPackages,
            locale: locale,
            localizedManagerName: localizedManagerName,
            priorityRank: priorityRank
        )
        return mergeSorted(
            baseProjections,
            remoteOnlyProjections,
            localizedManagerName: localizedManagerName,
            priorityRank: priorityRank
        )
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

    private func matchesQuery(
        _ member: Member,
        queryToken: String,
        normalizedQueryToken: String,
        managerParticipatesInSearch: (String) -> Bool
    ) -> Bool {
        LibraryPackageIndexMatchPolicy.matches(
            member.facts,
            queryToken: queryToken,
            normalizedQueryToken: normalizedQueryToken,
            managerID: nil,
            statusFilter: nil,
            pinnedOnly: false,
            managerParticipatesInSearch: managerParticipatesInSearch
        )
    }

    private func matchesFilters(
        _ package: PackageItem,
        managerID: String?,
        statusFilter: PackageStatus?,
        pinnedOnly: Bool
    ) -> Bool {
        LibraryPackageIndexMatchPolicy.matches(
            Self.facts(for: package),
            queryToken: "",
            normalizedQueryToken: "",
            managerID: managerID,
            statusFilter: statusFilter?.rawValue,
            pinnedOnly: pinnedOnly,
            managerParticipatesInSearch: { _ in true }
        )
    }

    private func consolidate(
        _ packages: [PackageItem],
        localizedManagerName: (String) -> String,
        priorityRank: ((String) -> Int)?
    ) -> ConsolidatedPackageItem? {
        ConsolidatedPackageItem.consolidate(
            packages,
            locale: locale,
            localizedManagerName: localizedManagerName,
            priorityRank: priorityRank
        ).first
    }

    private func mergeSorted(
        _ base: [ConsolidatedPackageItem],
        _ additional: [ConsolidatedPackageItem],
        localizedManagerName: (String) -> String,
        priorityRank: ((String) -> Int)?
    ) -> [ConsolidatedPackageItem] {
        guard !additional.isEmpty else { return base }
        var result: [ConsolidatedPackageItem] = []
        result.reserveCapacity(base.count + additional.count)
        var baseIndex = 0
        var additionalIndex = 0
        while baseIndex < base.count, additionalIndex < additional.count {
            if ConsolidatedPackageItem.isOrderedBefore(
                additional[additionalIndex],
                base[baseIndex],
                locale: locale,
                localizedManagerName: localizedManagerName,
                priorityRank: priorityRank
            ) {
                result.append(additional[additionalIndex])
                additionalIndex += 1
            } else {
                result.append(base[baseIndex])
                baseIndex += 1
            }
        }
        if baseIndex < base.count {
            result.append(contentsOf: base[baseIndex...])
        }
        if additionalIndex < additional.count {
            result.append(contentsOf: additional[additionalIndex...])
        }
        return result
    }

    private func prioritizeExactMatches(
        _ packages: [ConsolidatedPackageItem],
        queryBaseToken: String,
        normalizedQueryToken: String,
        queryHasQualifier: Bool
    ) -> [ConsolidatedPackageItem] {
        guard !queryBaseToken.isEmpty else { return packages }
        var qualifiedMatches: [ConsolidatedPackageItem] = []
        var baseMatches: [ConsolidatedPackageItem] = []
        var remainingPackages: [ConsolidatedPackageItem] = []
        qualifiedMatches.reserveCapacity(1)
        baseMatches.reserveCapacity(1)
        remainingPackages.reserveCapacity(packages.count)

        for projection in packages {
            if queryHasQualifier,
               projection.package.normalizedIdentityKey == normalizedQueryToken {
                qualifiedMatches.append(projection)
            } else if projection.package.normalizedBaseName == queryBaseToken {
                baseMatches.append(projection)
            } else {
                remainingPackages.append(projection)
            }
        }
        return qualifiedMatches + baseMatches + remainingPackages
    }

    static func facts(for package: PackageItem) -> LibraryPackageIndexMemberFacts {
        LibraryPackageIndexMemberFacts(
            managerID: package.managerId,
            status: package.status.rawValue,
            isPinned: package.pinned,
            normalizedIdentityKey: package.normalizedIdentityKey,
            managerSearchText: package.manager.lowercased(),
            summarySearchText: package.summary?.lowercased() ?? ""
        )
    }
}
