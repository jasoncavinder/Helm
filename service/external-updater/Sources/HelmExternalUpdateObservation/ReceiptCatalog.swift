import Foundation

/// file-info queries on some macOS versions match root-relative BOM paths
/// without applying a receipt's non-root install location. Inspect those exports
/// through pkgutil too; never infer absence by reading private receipt storage.
struct NativeReceiptCatalog {
    static let maximumBytes = 4 * 1024 * 1024
    var query: ([String], UInt64, Int) throws -> Data = { arguments, remaining, limit in
        try BoundedSystemQuery.run(executable: "/usr/sbin/pkgutil", arguments: ["--volume", "/"] + arguments,
                                   timeoutNanoseconds: min(remaining, 1_000_000_000), maximumBytes: limit)
    }

    func snapshot(target: URL, remaining: () throws -> UInt64) throws -> NativeInstallerReceiptObserver.Snapshot {
        var replies: [Data] = []
        var bytes = 0
        func read(_ arguments: [String], limit: Int) throws -> Data {
            let data = try query(arguments, remaining(), limit)
            guard !data.isEmpty, data.count <= limit, data.count <= Self.maximumBytes - bytes else {
                throw ObservationFailure.limitExceeded
            }
            bytes += data.count
            replies.append(data)
            _ = try remaining()
            return data
        }
        let list = try read(["--pkgs-plist"], limit: 256 * 1024)
        guard let identifiers = try PropertyListSerialization.propertyList(from: list, format: nil) as? [String],
              identifiers.count <= 1024, Set(identifiers).count == identifiers.count, identifiers.allSatisfy(Self.validIdentifier) else {
            throw ObservationFailure.unreadableManagerEvidence
        }
        let ids = identifiers.sorted()
        var claims = Set<String>()
        for offset in stride(from: 0, to: ids.count, by: 32) {
            let batch = Array(ids[offset..<min(offset + 32, ids.count)])
            let data = try read(batch.flatMap { ["--pkg-info-plist", $0] }, limit: 64 * 1024)
            let documents = try NativeInstallerReceiptObserver.documents(data, count: batch.count)
            for (identifier, document) in zip(batch, documents) {
                let info = try Self.record(document, identifier: identifier)
                let location = try Self.location(info)
                guard location != "/", Self.overlaps(location, target.path) else { continue }
                let exported = try read(["--export-plist", identifier], limit: 2 * 1024 * 1024)
                let receipt = try Self.record(exported, identifier: identifier)
                guard try Self.location(receipt) == location,
                      receipt["pkg-version"] as? String == info["pkg-version"] as? String,
                      receipt["install-time"] as? NSNumber == info["install-time"] as? NSNumber,
                      let paths = receipt["paths"] as? [String: Any], paths.count <= 100_000 else {
                    throw ObservationFailure.unreadableManagerEvidence
                }
                for (relative, raw) in paths {
                    let components = try Self.components(relative, absoluteAllowed: false)
                    guard let metadata = raw as? [String: Any], metadata["pkgid"] as? String == identifier else {
                        throw ObservationFailure.unreadableManagerEvidence
                    }
                    let installed = location + (components.isEmpty ? "" : "/" + components.joined(separator: "/"))
                    if installed == target.path || installed.hasPrefix(target.path + "/") { claims.insert(identifier) }
                }
            }
        }
        _ = try remaining()
        return .init(replies: replies, identifiers: claims.sorted())
    }

    private static func validIdentifier(_ id: String) -> Bool {
        let alphanumeric: (UInt8) -> Bool = { (65...90).contains($0) || (97...122).contains($0) || (48...57).contains($0) }
        return !id.isEmpty && id.utf8.count <= 255 && id.utf8.first.map(alphanumeric) == true
            && id.utf8.allSatisfy { alphanumeric($0) || [45, 46, 95, 43].contains($0) }
    }

    private static func record(_ data: Data, identifier: String) throws -> [String: Any] {
        guard let record = try PropertyListSerialization.propertyList(from: data, format: nil) as? [String: Any],
              record["pkgid"] as? String == identifier, record["volume"] as? String == "/",
              let version = record["pkg-version"] as? String, !version.isEmpty, version.utf8.count <= 255,
              record["install-time"] is NSNumber else { throw ObservationFailure.unreadableManagerEvidence }
        return record
    }

    private static func location(_ record: [String: Any]) throws -> String {
        guard let path = record["install-location"] as? String else { throw ObservationFailure.unreadableManagerEvidence }
        return "/" + (try components(path, absoluteAllowed: true)).joined(separator: "/")
    }

    private static func components(_ path: String, absoluteAllowed: Bool) throws -> [String] {
        guard path.utf8.count <= 4096, absoluteAllowed || !path.hasPrefix("/"),
              !path.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) }) else {
            throw ObservationFailure.unreadableManagerEvidence
        }
        let parts = path.split(separator: "/")
        guard !parts.contains("..") else { throw ObservationFailure.unreadableManagerEvidence }
        return parts.filter { $0 != "." }.map(String.init)
    }

    private static func overlaps(_ first: String, _ second: String) -> Bool {
        first == second || first.hasPrefix(second + "/") || second.hasPrefix(first + "/")
    }
}
