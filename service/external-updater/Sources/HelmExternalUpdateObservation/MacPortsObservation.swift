import Darwin
import Foundation

/// Denial-only standard-prefix registry observation. Missing claims never prove
/// standalone ownership, and unsupported/busy registries are not empty results.
struct NativeMacPortsObserver {
    static let registry = URL(fileURLWithPath: "/opt/local/var/macports/registry/registry.db", isDirectory: false)
    static let maximumRows = 100_000
    static let maximumBytes = 4 * 1024 * 1024
    static let query = """
        PRAGMA query_only=ON;
        PRAGMA trusted_schema=OFF;
        SELECT 'version' AS kind, value AS path, NULL AS actual_path, NULL AS active
        FROM metadata WHERE key = 'version'
        UNION ALL SELECT 'file', path, actual_path, active FROM files LIMIT 100002;
        """
    let registry: URL
    var read: ([String]) throws -> Data = {
        try BoundedSystemQuery.run(executable: "/usr/bin/sqlite3", arguments: $0,
                                   timeoutNanoseconds: 1_000_000_000, maximumBytes: maximumBytes)
    }

    init(registry: URL = Self.registry) { self.registry = registry }

    struct Snapshot: Equatable {
        let filesystem: [String: FileIdentity]
        let reply: Data
        let claims: [String]
    }

    func snapshot(target: URL) throws -> Snapshot {
        let before = try capture()
        guard before[registry.path] != nil else {
            return Snapshot(filesystem: before, reply: Data(), claims: [])
        }
        // immutable prevents SQLite from creating journals or shared-memory
        // files. A nonempty WAL/journal is rejected, never silently ignored.
        var uri = URLComponents(url: registry, resolvingAgainstBaseURL: false)
        uri?.queryItems = [URLQueryItem(name: "mode", value: "ro"), URLQueryItem(name: "immutable", value: "1")]
        guard let database = uri?.string else { throw ObservationFailure.invalidPath }
        let data = try read(["-batch", "-safe", "-readonly", "-init", "/dev/null", "-json", database, Self.query])
        let claims = try Self.claims(data, target: target.path)
        guard before == (try capture()) else { throw ObservationFailure.changedDuringObservation }
        return Snapshot(filesystem: before, reply: data, claims: claims)
    }

    private struct Row: Decodable {
        let kind: String
        let path: String
        let actual_path: String?
        let active: Int?
    }

    static func claims(_ data: Data, target: String) throws -> [String] {
        guard !data.isEmpty, data.count <= maximumBytes else { throw ObservationFailure.limitExceeded }
        let rows = try JSONDecoder().decode([Row].self, from: data)
        guard let version = rows.first, version.kind == "version", version.path == "1.215",
              version.actual_path == nil, version.active == nil else { throw ObservationFailure.unreadableManagerEvidence }
        guard rows.count <= maximumRows + 1 else { throw ObservationFailure.limitExceeded }
        let target = try denialPath(target)
        var claims = Set<String>()
        for row in rows.dropFirst() {
            guard row.kind == "file", row.active == 0 || row.active == 1 else {
                throw ObservationFailure.unreadableManagerEvidence
            }
            let path = try denialPath(row.path)
            let actual = try row.actual_path.map(denialPath)
            guard row.active != 1 || actual != nil else { throw ObservationFailure.unreadableManagerEvidence }
            // Retained/inactive records are conservative denial signals too;
            // neither spelling is an assertion about current file identity.
            for (raw, normalized) in [(row.path, path), (row.actual_path ?? row.path, actual ?? path)] {
                if normalized == target || normalized.hasPrefix(target + "/") { claims.insert(raw) }
            }
        }
        return claims.sorted()
    }

    static func denialPath(_ path: String) throws -> String {
        guard path.hasPrefix("/"), path.utf8.count <= 4096, !path.contains("//"),
              !path.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) }),
              !path.split(separator: "/").contains(where: { $0 == "." || $0 == ".." }) else {
            throw ObservationFailure.unreadableManagerEvidence
        }
        return path.folding(options: .caseInsensitive, locale: Locale(identifier: "en_US_POSIX"))
            .precomposedStringWithCanonicalMapping
    }

    private func capture() throws -> [String: FileIdentity] {
        _ = try Self.denialPath(registry.path)
        guard registry.isFileURL else { throw ObservationFailure.invalidPath }
        var identities: [String: FileIdentity] = [:]
        var path = URL(fileURLWithPath: "/", isDirectory: true)
        for component in registry.path.split(separator: "/") {
            path.appendPathComponent(String(component), isDirectory: false)
            let descriptor = open(path.path, O_RDONLY | O_CLOEXEC | O_NOFOLLOW_ANY | O_NONBLOCK)
            if descriptor < 0 {
                guard errno == ENOENT else { throw ObservationFailure.unreadableManagerEvidence }
                guard try sidecars().isEmpty else { throw ObservationFailure.unreadableManagerEvidence }
                return identities
            }
            defer { close(descriptor) }
            let identity = try FileIdentity.read(descriptor: descriptor)
            if path == registry {
                guard identity.isRegular, identity.size > 0, identity.size <= 64 * 1024 * 1024 else {
                    throw ObservationFailure.unreadableManagerEvidence
                }
            } else {
                guard identity.isDirectory else { throw ObservationFailure.unreadableManagerEvidence }
            }
            identities[path.path] = identity
        }
        identities.merge(try sidecars()) { _, latest in latest }
        return identities
    }

    private func sidecars() throws -> [String: FileIdentity] {
        var identities: [String: FileIdentity] = [:]
        for suffix in ["-wal", "-shm", "-journal"] {
            let path = registry.path + suffix
            let descriptor = open(path, O_RDONLY | O_CLOEXEC | O_NOFOLLOW_ANY | O_NONBLOCK)
            if descriptor < 0 {
                guard errno == ENOENT else { throw ObservationFailure.unreadableManagerEvidence }
                continue
            }
            defer { close(descriptor) }
            let identity = try FileIdentity.read(descriptor: descriptor)
            guard identity.isRegular, identity.size >= 0,
                  suffix == "-shm" ? identity.size <= 1024 * 1024 : identity.size == 0 else {
                throw ObservationFailure.unreadableManagerEvidence
            }
            identities[path] = identity
        }
        return identities
    }
}
