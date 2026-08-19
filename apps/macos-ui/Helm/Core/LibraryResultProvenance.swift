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
    case managerSearch = "manager_search"
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

enum PackageResultProvenanceBoundary {
    case managerSnapshot
    case localCacheSearch
    case directSearch
}

struct PackageResultProvenance: Codable, Hashable {
    static let supportedSchemaVersion = 1

    let schemaVersion: Int
    let origin: PackageResultOrigin
    let discoverySource: PackageResultDiscoverySource
    let sourceManager: String
    let originatingQuery: String?
    private let isStructurallyValid: Bool

    private enum CodingKeys: String, CodingKey {
        case schemaVersion
        case origin
        case discoverySource
        case sourceManager
        case originatingQuery
    }

    init(from decoder: Decoder) throws {
        guard let container = try? decoder.container(keyedBy: CodingKeys.self) else {
            schemaVersion = 0
            origin = .unknown
            discoverySource = .unknown
            sourceManager = ""
            originatingQuery = nil
            isStructurallyValid = false
            return
        }

        let decodedSchemaVersion = try? container.decode(Int.self, forKey: .schemaVersion)
        let decodedOrigin = try? container.decode(PackageResultOrigin.self, forKey: .origin)
        let decodedDiscoverySource = try? container.decode(
            PackageResultDiscoverySource.self,
            forKey: .discoverySource
        )
        let decodedSourceManager = try? container.decode(String.self, forKey: .sourceManager)
        let query = Self.decodeOptional(String.self, forKey: .originatingQuery, from: container)

        schemaVersion = decodedSchemaVersion ?? 0
        origin = decodedOrigin ?? .unknown
        discoverySource = decodedDiscoverySource ?? .unknown
        sourceManager = decodedSourceManager ?? ""
        originatingQuery = query.value
        isStructurallyValid = decodedSchemaVersion != nil
            && decodedOrigin != nil
            && decodedDiscoverySource != nil
            && decodedSourceManager != nil
            && query.isValid
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(schemaVersion, forKey: .schemaVersion)
        try container.encode(origin, forKey: .origin)
        try container.encode(discoverySource, forKey: .discoverySource)
        try container.encode(sourceManager, forKey: .sourceManager)
        try container.encodeIfPresent(originatingQuery, forKey: .originatingQuery)
    }

    func validated(for managerId: String, at boundary: PackageResultProvenanceBoundary) -> Self? {
        let canonicalSource = sourceManager.trimmingCharacters(in: .whitespacesAndNewlines)
        let canonicalManager = managerId.trimmingCharacters(in: .whitespacesAndNewlines)
        guard isStructurallyValid,
              schemaVersion == Self.supportedSchemaVersion,
              !sourceManager.isEmpty,
              sourceManager == canonicalSource,
              managerId == canonicalManager,
              sourceManager == managerId,
              Self.isCanonicalManagerID(managerId),
              origin != .unknown,
              discoverySource != .unknown else {
            return nil
        }

        let canonicalQuery = originatingQuery?.trimmingCharacters(in: .whitespacesAndNewlines)
        switch (origin, discoverySource) {
        case (.local, .managerSnapshot):
            guard originatingQuery == nil else { return nil }
        case (.localCache, .catalogSync):
            guard originatingQuery == nil else { return nil }
        case (.localCache, .managerSearch), (.remote, .managerSearch), (.deferred, .managerSearch):
            guard let originatingQuery,
                  !originatingQuery.isEmpty,
                  originatingQuery == canonicalQuery else {
                return nil
            }
        default:
            return nil
        }

        switch boundary {
        case .managerSnapshot:
            guard origin == .local, discoverySource == .managerSnapshot else { return nil }
        case .localCacheSearch:
            guard origin == .localCache,
                  discoverySource == .catalogSync || discoverySource == .managerSearch else {
                return nil
            }
        case .directSearch:
            guard origin == .remote || origin == .deferred,
                  discoverySource == .managerSearch else {
                return nil
            }
        }
        return self
    }

    private static func isCanonicalManagerID(_ managerID: String) -> Bool {
        let allowed = CharacterSet(charactersIn: "abcdefghijklmnopqrstuvwxyz0123456789_")
        return managerID.rangeOfCharacter(from: allowed.inverted) == nil
            && managerID.first != "_"
            && managerID.last != "_"
            && !managerID.contains("__")
    }

    private static func decodeOptional<Value: Decodable>(
        _ type: Value.Type,
        forKey key: CodingKeys,
        from container: KeyedDecodingContainer<CodingKeys>
    ) -> (value: Value?, isValid: Bool) {
        guard container.contains(key) else { return (nil, true) }
        if (try? container.decodeNil(forKey: key)) == true {
            return (nil, true)
        }
        guard let value = try? container.decode(Value.self, forKey: key) else {
            return (nil, false)
        }
        return (value, true)
    }
}
