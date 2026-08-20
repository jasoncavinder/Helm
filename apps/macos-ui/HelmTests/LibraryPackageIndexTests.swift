import Foundation
import XCTest

final class LibraryPackageIndexTests: XCTestCase {
    func testSameIDOverlayEnrichesLocalMemberWithoutReplacingIdentityOrProvenance() throws {
        let localProvenance = try provenance(origin: "local", discovery: "manager_snapshot")
        let remoteProvenance = try provenance(origin: "remote", discovery: "manager_search")
        let local = package(
            id: "homebrew_formula:ripgrep",
            name: "ripgrep",
            version: "14.1.0",
            managerID: "homebrew_formula",
            provenance: localProvenance
        )
        let remote = package(
            id: local.id,
            name: "ripgrep",
            version: "",
            latestVersion: "15.0.0",
            managerID: "homebrew_formula",
            summary: "Fast recursive search",
            provenance: remoteProvenance,
            status: .available
        )
        let rows = makeIndex([local]).filteredPackages(
            query: "ripgrep",
            managerID: nil,
            statusFilter: .installed,
            pinnedOnly: false,
            overlay: LibraryPackageSearchOverlay(packages: [remote]),
            managerParticipatesInSearch: { _ in true },
            localizedManagerName: { $0 }
        )

        let row = try XCTUnwrap(rows.first)
        XCTAssertEqual(rows.count, 1)
        XCTAssertEqual(row.package.id, local.id)
        XCTAssertEqual(row.memberPackages.map(\.id), [local.id])
        XCTAssertEqual(row.package.summary, remote.summary)
        XCTAssertEqual(row.package.latestVersion, remote.latestVersion)
        XCTAssertEqual(row.package.status, .upgradable)
        XCTAssertEqual(row.package.resultProvenance?.origin, .local)
        XCTAssertEqual(row.memberPackages.first?.resultProvenance?.origin, .local)
    }

    func testNewIdentityOverlayKeepsRemoteMemberAndProvenance() throws {
        let remote = package(
            id: "cargo:fd",
            name: "fd",
            version: "",
            managerID: "cargo",
            summary: "Friendly file search",
            provenance: try provenance(
                origin: "remote",
                discovery: "manager_search",
                managerID: "cargo"
            ),
            status: .available
        )
        let local = package(
            id: "homebrew_formula:ripgrep",
            name: "ripgrep",
            version: "14.1.0",
            managerID: "homebrew_formula",
            summary: "Recursive file search"
        )
        let rows = makeIndex([local]).filteredPackages(
            query: "search",
            managerID: nil,
            statusFilter: nil,
            pinnedOnly: false,
            overlay: LibraryPackageSearchOverlay(packages: [remote]),
            managerParticipatesInSearch: { _ in true },
            managerIsEnabled: { _ in true },
            localizedManagerName: { $0 }
        )

        let remoteRow = try XCTUnwrap(rows.first(where: { $0.package.id == remote.id }))
        XCTAssertEqual(rows.count, 2)
        XCTAssertEqual(remoteRow.memberPackages.map(\.id), [remote.id])
        XCTAssertEqual(remoteRow.package.resultProvenance?.origin, .remote)

        let enabledOnlyRows = makeIndex([local]).filteredPackages(
            query: "search",
            managerID: nil,
            statusFilter: nil,
            pinnedOnly: false,
            overlay: LibraryPackageSearchOverlay(packages: [remote]),
            managerParticipatesInSearch: { _ in true },
            managerIsEnabled: { $0 != "cargo" },
            localizedManagerName: { $0 }
        )
        XCTAssertEqual(enabledOnlyRows.map(\.package.id), [local.id])
    }

    func testDifferentIDOverlayJoinsMatchingIdentityAndKeepsBothMembers() throws {
        let local = package(
            id: "homebrew_formula:fd",
            name: "fd",
            version: "10.2.0",
            managerID: "homebrew_formula",
            provenance: try provenance(origin: "local", discovery: "manager_snapshot")
        )
        let remote = package(
            id: "cargo:fd",
            name: "fd",
            version: "",
            managerID: "cargo",
            summary: "Friendly file search",
            provenance: try provenance(
                origin: "remote",
                discovery: "manager_search",
                managerID: "cargo"
            ),
            status: .available
        )
        let rows = makeIndex([local]).filteredPackages(
            query: "fd",
            managerID: nil,
            statusFilter: nil,
            pinnedOnly: false,
            overlay: LibraryPackageSearchOverlay(packages: [remote]),
            managerParticipatesInSearch: { _ in true },
            localizedManagerName: { $0 }
        )

        let row = try XCTUnwrap(rows.first)
        XCTAssertEqual(rows.count, 1)
        XCTAssertEqual(Set(row.memberPackages.map(\.id)), Set([local.id, remote.id]))
        XCTAssertEqual(
            row.memberPackages.first(where: { $0.id == remote.id })?.resultProvenance?.origin,
            .remote
        )
    }

    func testFutureOverlayProvenanceFailsClosedWithoutDroppingResult() throws {
        let futureProvenance = try provenance(
            schemaVersion: PackageResultProvenance.supportedSchemaVersion + 1,
            origin: "local_cache",
            discovery: "manager_search",
            managerID: "future_manager"
        )
        let validatedProvenance = futureProvenance.validated(
            for: "future_manager",
            at: .localCacheSearch
        )
        XCTAssertNil(validatedProvenance)

        let remote = package(
            id: "future_manager:search-tool",
            name: "search-tool",
            version: "",
            managerID: "future_manager",
            provenance: validatedProvenance,
            status: .available
        )
        let rows = makeIndex([]).filteredPackages(
            query: "search",
            managerID: nil,
            statusFilter: nil,
            pinnedOnly: false,
            overlay: LibraryPackageSearchOverlay(packages: [remote]),
            managerParticipatesInSearch: { _ in true },
            managerIsEnabled: { _ in true },
            localizedManagerName: { $0 }
        )

        XCTAssertEqual(rows.first?.memberPackages.map(\.id), [remote.id])
        XCTAssertNil(rows.first?.package.resultProvenance)
    }

    func testOverlayPreservesPinnedAndMergedStatusFiltering() {
        let local = package(
            id: "homebrew_formula:ripgrep",
            name: "ripgrep",
            version: "14.1.0",
            managerID: "homebrew_formula",
            pinned: true
        )
        let remote = package(
            id: local.id,
            name: local.name,
            version: "",
            latestVersion: "15.0.0",
            managerID: local.managerId,
            status: .available
        )
        let index = makeIndex([local])
        let overlay = LibraryPackageSearchOverlay(packages: [remote])

        XCTAssertTrue(
            index.filteredPackages(
                query: "ripgrep",
                managerID: nil,
                statusFilter: .upgradable,
                pinnedOnly: false,
                overlay: overlay,
                managerParticipatesInSearch: { _ in true },
                localizedManagerName: { $0 }
            ).isEmpty
        )
        XCTAssertEqual(
            index.filteredPackages(
                query: "ripgrep",
                managerID: "homebrew_formula",
                statusFilter: .installed,
                pinnedOnly: true,
                overlay: overlay,
                managerParticipatesInSearch: { _ in true },
                localizedManagerName: { $0 }
            ).map(\.package.id),
            [local.id]
        )
    }

    func testMultiMemberGroupReconsolidatesExactFilteredMembers() throws {
        let homebrew = package(
            id: "homebrew_formula:ripgrep",
            name: "ripgrep",
            version: "14.1.0",
            latestVersion: "15.0.0",
            managerID: "homebrew_formula"
        )
        let cargo = package(
            id: "cargo:ripgrep",
            name: "ripgrep",
            version: "14.1.0",
            managerID: "cargo",
            summary: "Cargo summary",
            pinned: true
        )
        let index = makeIndex([homebrew, cargo])

        let allMembers = try XCTUnwrap(
            index.filteredPackages(
                query: "ripgrep",
                managerID: nil,
                statusFilter: nil,
                pinnedOnly: false,
                managerParticipatesInSearch: { _ in true },
                localizedManagerName: { $0 }
            ).first
        )
        XCTAssertEqual(Set(allMembers.memberPackages.map(\.id)), Set([homebrew.id, cargo.id]))
        XCTAssertEqual(allMembers.package.summary, cargo.summary)

        let managerRow = try XCTUnwrap(
            index.filteredPackages(
                query: "ripgrep",
                managerID: "cargo",
                statusFilter: nil,
                pinnedOnly: false,
                managerParticipatesInSearch: { _ in true },
                localizedManagerName: { $0 }
            ).first
        )
        XCTAssertEqual(managerRow.memberPackages.map(\.id), [cargo.id])
        XCTAssertEqual(managerRow.package.id, cargo.id)
        XCTAssertEqual(managerRow.package.summary, cargo.summary)

        let statusRow = try XCTUnwrap(
            index.filteredPackages(
                query: "ripgrep",
                managerID: nil,
                statusFilter: .upgradable,
                pinnedOnly: false,
                managerParticipatesInSearch: { _ in true },
                localizedManagerName: { $0 }
            ).first
        )
        XCTAssertEqual(statusRow.memberPackages.map(\.id), [homebrew.id])
        XCTAssertEqual(statusRow.package.id, homebrew.id)

        let pinnedRow = try XCTUnwrap(
            index.filteredPackages(
                query: "ripgrep",
                managerID: nil,
                statusFilter: .installed,
                pinnedOnly: true,
                managerParticipatesInSearch: { _ in true },
                localizedManagerName: { $0 }
            ).first
        )
        XCTAssertEqual(pinnedRow.memberPackages.map(\.id), [cargo.id])
        XCTAssertEqual(pinnedRow.package.id, cargo.id)
    }

    func testLocaleAwarePrimaryOrderingPlacesAetherBeforeZulu() {
        let packages = [
            package(id: "cargo:zulu", name: "Zulu", version: "", managerID: "cargo"),
            package(id: "cargo:aether", name: "Æther", version: "", managerID: "cargo"),
        ]
        let locale = Locale(identifier: "en_US")
        let consolidated = ConsolidatedPackageItem.consolidate(
            packages,
            locale: locale,
            localizedManagerName: { $0 },
            priorityRank: { _ in 0 }
        )
        let rows = LibraryPackageIndex(
            packages: packages,
            localeIdentifier: locale.identifier,
            localizedManagerName: { $0 },
            priorityRank: { _ in 0 }
        ).filteredPackages(
            query: "",
            managerID: nil,
            statusFilter: nil,
            pinnedOnly: false,
            managerParticipatesInSearch: { _ in true },
            localizedManagerName: { $0 }
        )

        XCTAssertEqual(consolidated.map { $0.package.name }, ["Æther", "Zulu"])
        XCTAssertEqual(rows.map { $0.package.name }, ["Æther", "Zulu"])
    }

    func testIdenticalSnapshotRetainsRealIndexCacheGeneration() throws {
        XCTAssertFalse(LibraryPackageIndexInvalidationPolicy.managerEnablementChanged(
            previous: [:], current: ["cargo": true]
        ))
        XCTAssertTrue(LibraryPackageIndexInvalidationPolicy.managerEnablementChanged(
            previous: ["cargo": true], current: ["cargo": false]
        ))
        let firstPackage = package(
            id: "cargo:ripgrep",
            name: "ripgrep",
            version: "14.1.0",
            managerID: "cargo"
        )
        let secondPackage = package(
            id: "cargo:fd",
            name: "fd",
            version: "10.2.0",
            managerID: "cargo"
        )
        var sourceRevision: UInt64 = 7
        var cache = LibraryPackageIndexCache()
        _ = cache.resolve(
            packages: [firstPackage, secondPackage],
            sourceRevision: sourceRevision,
            localeIdentifier: "en_US",
            localizedManagerName: { $0 },
            priorityRank: { _ in 0 }
        )
        let initialGeneration = cache.generation
        let reorderedSnapshot = [secondPackage, firstPackage]

        XCTAssertFalse(
            PackageSnapshotPublicationPolicy.shouldPublish(
                current: [firstPackage, secondPackage],
                replacement: reorderedSnapshot
            )
        )
        _ = cache.resolve(
            packages: reorderedSnapshot,
            sourceRevision: sourceRevision,
            localeIdentifier: "en_US",
            localizedManagerName: { $0 },
            priorityRank: { _ in 0 }
        )
        XCTAssertEqual(cache.generation, initialGeneration)

        let changedPackage = package(
            id: firstPackage.id,
            name: firstPackage.name,
            version: firstPackage.version,
            managerID: firstPackage.managerId,
            summary: "Changed summary"
        )
        XCTAssertTrue(
            PackageSnapshotPublicationPolicy.shouldPublish(
                current: [firstPackage, secondPackage],
                replacement: [changedPackage, secondPackage]
            )
        )
        XCTAssertFalse(
            PackageSnapshotPublicationPolicy.areOrderedSemanticallyEquivalent(
                [firstPackage],
                [changedPackage]
            )
        )
        let changedProvenance = package(
            id: firstPackage.id,
            name: firstPackage.name,
            version: firstPackage.version,
            managerID: firstPackage.managerId,
            provenance: try provenance(origin: "local", discovery: "manager_snapshot")
        )
        XCTAssertFalse(
            PackageSnapshotPublicationPolicy.areOrderedSemanticallyEquivalent(
                [firstPackage],
                [changedProvenance]
            )
        )
        sourceRevision += 1
        _ = cache.resolve(
            packages: [changedPackage, secondPackage],
            sourceRevision: sourceRevision,
            localeIdentifier: "en_US",
            localizedManagerName: { $0 },
            priorityRank: { _ in 0 }
        )
        XCTAssertEqual(cache.generation, initialGeneration + 1)
    }

    func testFilterChangesReuseRevisionedSearchOverlay() {
        let remote = package(
            id: "cargo:ripgrep",
            name: "ripgrep",
            version: "",
            managerID: "cargo",
            summary: "Fast search",
            status: .available
        )
        var cache = LibraryPackageSearchOverlayCache()
        let overlay = cache.resolve(packages: [remote], sourceRevision: 4)
        let generation = cache.generation
        let index = makeIndex([])
        _ = index.filteredPackages(
            query: "ripgrep",
            managerID: nil,
            statusFilter: nil,
            pinnedOnly: false,
            overlay: overlay,
            managerParticipatesInSearch: { _ in true },
            localizedManagerName: { $0 }
        )
        let reusedOverlay = cache.resolve(packages: [remote], sourceRevision: 4)
        _ = index.filteredPackages(
            query: "search",
            managerID: "cargo",
            statusFilter: .available,
            pinnedOnly: false,
            overlay: reusedOverlay,
            managerParticipatesInSearch: { _ in true },
            localizedManagerName: { $0 }
        )

        XCTAssertEqual(cache.generation, generation)
    }

    private func makeIndex(_ packages: [PackageItem]) -> LibraryPackageIndex {
        LibraryPackageIndex(
            packages: packages,
            localeIdentifier: "en_US",
            localizedManagerName: { $0 },
            priorityRank: { _ in 0 }
        )
    }

    private func package(
        id: String,
        name: String,
        version: String,
        latestVersion: String? = nil,
        managerID: String,
        summary: String? = nil,
        pinned: Bool = false,
        provenance: PackageResultProvenance? = nil,
        status: PackageStatus? = nil
    ) -> PackageItem {
        PackageItem(
            id: id,
            name: name,
            version: version,
            latestVersion: latestVersion,
            managerId: managerID,
            manager: managerID,
            summary: summary,
            pinned: pinned,
            resultProvenance: provenance,
            status: status
        )
    }

    private func provenance(
        schemaVersion: Int = PackageResultProvenance.supportedSchemaVersion,
        origin: String,
        discovery: String,
        managerID: String = "homebrew_formula"
    ) throws -> PackageResultProvenance {
        let query = discovery == "manager_search" ? #", "originatingQuery": "ripgrep""# : ""
        let json = """
        {
          "schemaVersion": \(schemaVersion),
          "origin": "\(origin)",
          "discoverySource": "\(discovery)",
          "sourceManager": "\(managerID)"\(query)
        }
        """
        return try JSONDecoder().decode(
            PackageResultProvenance.self,
            from: try XCTUnwrap(json.data(using: .utf8))
        )
    }
}
