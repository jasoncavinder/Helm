import Foundation

struct ProductionLibraryManagerCapability: Equatable {
    let managerID: String
    let enabled: Bool
    let supportsPackageInstall: Bool
    let supportsPackageUpgrade: Bool
    let supportsRemoteSearch: Bool
    let isUninstalling: Bool
}

struct ProductionLibraryAppUpdateCapability: Equatable {
    let updateAvailable: Bool
    let canCheckForUpdates: Bool
}

struct ProductionLibraryTableProjectionIdentity: Equatable {
    let sourceFingerprint: Int
    let managerConstraint: String?
    let selectedPackageID: String?
    let managerPreferences: [String: String]
    let pinActionPackageIDs: Set<String>
    let upgradeActionPackageIDs: Set<String>
    let installablePackageNames: Set<String>
    let installActionPackageNames: Set<String>
    let managerCapabilities: [ProductionLibraryManagerCapability]
    let appUpdateCapability: ProductionLibraryAppUpdateCapability
    let networkOperationsAvailable: Bool
    let locale: String
}

struct LibraryTableProjectionResolution {
    let snapshot: LibraryTableSnapshot
    let cacheHit: Bool
}

struct ProductionLibraryTableProjectionCache {
    private let namespace: String
    private var identity: ProductionLibraryTableProjectionIdentity?
    private(set) var generation: UInt64 = 0
    private(set) var snapshot: LibraryTableSnapshot

    init(namespace: String) {
        self.namespace = namespace
        snapshot = .empty(namespace: namespace)
    }

    mutating func resolve(
        identity: ProductionLibraryTableProjectionIdentity,
        project: (LibraryTableModelRevision) -> LibraryTableSnapshot
    ) -> LibraryTableProjectionResolution {
        if self.identity == identity {
            return LibraryTableProjectionResolution(
                snapshot: snapshot,
                cacheHit: true
            )
        }

        var nextGeneration = generation &+ 1
        if nextGeneration == 0 {
            nextGeneration = 1
        }
        let revision = LibraryTableModelRevision(
            namespace: namespace,
            generation: nextGeneration
        )
        let nextSnapshot = project(revision)
        self.identity = identity
        generation = nextGeneration
        snapshot = nextSnapshot
        return LibraryTableProjectionResolution(
            snapshot: nextSnapshot,
            cacheHit: false
        )
    }

    mutating func invalidate() {
        identity = nil
    }
}
