import Foundation

enum PackageResultOrigin: String, Codable, Hashable {
    case local
    case localCache = "local_cache"
    case remote
    case deferred
    case unknown

    init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        self = Self(rawValue: try container.decode(String.self)) ?? .unknown
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(rawValue)
    }
}

enum PackageResultDiscoverySource: String, Codable, Hashable {
    case managerSnapshot = "manager_snapshot"
    case catalogSync = "catalog_sync"
    case remoteSearch = "remote_search"
    case unknown

    init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        self = Self(rawValue: try container.decode(String.self)) ?? .unknown
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(rawValue)
    }
}

struct PackageResultProvenance: Codable, Hashable {
    static let supportedSchemaVersion = 1

    let schemaVersion: Int
    let origin: PackageResultOrigin
    let discoverySource: PackageResultDiscoverySource
    let sourceManager: String
    let originatingQuery: String?
    let observedAtUnix: UInt64?

    func validated(for managerId: String) -> Self? {
        let normalizedSource = sourceManager.trimmingCharacters(in: .whitespacesAndNewlines)
        let normalizedManager = managerId.trimmingCharacters(in: .whitespacesAndNewlines)
        guard schemaVersion == Self.supportedSchemaVersion,
              !normalizedSource.isEmpty,
              normalizedSource == normalizedManager,
              origin != .unknown,
              discoverySource != .unknown else {
            return nil
        }

        let normalizedQuery = originatingQuery?.trimmingCharacters(in: .whitespacesAndNewlines)
        switch (origin, discoverySource) {
        case (.local, .managerSnapshot):
            guard normalizedQuery?.isEmpty != false else { return nil }
        case (.localCache, .catalogSync):
            guard normalizedQuery?.isEmpty != false else { return nil }
        case (.localCache, .remoteSearch), (.remote, .remoteSearch), (.deferred, .remoteSearch):
            guard normalizedQuery?.isEmpty == false else { return nil }
        default:
            return nil
        }
        return self
    }
}
