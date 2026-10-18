import CryptoKit
import Foundation

/// Compact denial evidence for root-relative pkgutil --files output. Retain a
/// digest of every byte, not just matching paths, without retaining SDK-sized
/// listings in each target/review. This does not resolve aliases or scripts.
struct NativeRootReceiptPayload: Encodable, Equatable {
    static let maximumBytes = 8 * 1024 * 1024
    static let maximumTotalBytes = 32 * 1024 * 1024
    static let maximumPaths = 100_000

    let domain = "helm.external-updater.root-receipt-payload.v1"
    let identifier: String
    let byteCount: Int
    let pathCount: Int
    let digest: Data
    let claimsTarget: Bool

    init(identifier: String, data: Data, target: URL, remaining: () throws -> UInt64) throws {
        guard data.count <= Self.maximumBytes else { throw ObservationFailure.limitExceeded }
        // Require the complete line terminator. Empty successful output is a
        // payload-less receipt, not permission to adopt or ignore install hooks.
        guard data.isEmpty || data.last == 10, let text = String(data: data, encoding: .utf8) else {
            throw ObservationFailure.unreadableManagerEvidence
        }
        var count = 0
        for byte in text.utf8 where byte == 10 {
            count += 1
            guard count <= Self.maximumPaths else { throw ObservationFailure.limitExceeded }
        }
        let lines = text.isEmpty ? [] : text.utf8.dropLast().split(separator: 10, omittingEmptySubsequences: false)
        let targetParts = target.path.split(separator: "/").map { NativeReceiptCatalog.denialPath(String($0)) }
        var claims = false
        for (index, line) in lines.enumerated() {
            if index % 256 == 0 { _ = try remaining() }
            // Parent directories such as /Applications are shared scaffolding,
            // not claims on every app they contain. Historical children count
            // even when absent from the currently inspected filesystem tree.
            if try Self.matches(line, targetParts: targetParts) { claims = true }
        }
        _ = try remaining()
        self.identifier = identifier
        byteCount = data.count
        pathCount = lines.count
        digest = Data(SHA256.hash(data: data))
        claimsTarget = claims
        _ = try remaining()
    }

    private static func matches(_ bytes: String.UTF8View.SubSequence, targetParts: [String]) throws -> Bool {
        // UTF-8 was validated before splitting. Check ASCII syntax without
        // allocating/folding every long SDK path, including unrelated paths.
        guard !bytes.isEmpty, bytes.count <= 4096, bytes.first != 47,
              !bytes.contains(where: { $0 < 32 || $0 == 127 }) else { throw ObservationFailure.unreadableManagerEvidence }
        if bytes.contains(where: { $0 >= 128 }) {
            guard let text = String(bytes: bytes, encoding: .utf8),
                  !text.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) }) else {
                throw ObservationFailure.unreadableManagerEvidence
            }
        }
        let parts = bytes.split(separator: 47).filter { !$0.elementsEqual(".".utf8) }
        guard !parts.contains(where: { $0.elementsEqual("..".utf8) }) else { throw ObservationFailure.unreadableManagerEvidence }
        guard parts.count >= targetParts.count else { return false }
        return try zip(parts, targetParts).allSatisfy { part, target in
            guard let text = String(bytes: part, encoding: .utf8) else { throw ObservationFailure.unreadableManagerEvidence }
            return NativeReceiptCatalog.denialPath(text) == target
        }
    }
}
