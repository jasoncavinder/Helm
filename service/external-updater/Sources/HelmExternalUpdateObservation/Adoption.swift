import CExternalUpdatePolicy
import Foundation

/// Untrusted per-app intent, never native evidence or update consent.
public struct ExternalAdoptionRequest: Codable {
    public let schemaVersion: UInt32
    public let consentId: String
    public let targetPath: String
    public let expectedBundleIdentifier: String
    public let expectedInstalledBuild: String

    public init(targetPath: String, bundleIdentifier: String, installedBuild: String) {
        schemaVersion = 1
        consentId = UUID().uuidString.lowercased()
        self.targetPath = targetPath
        expectedBundleIdentifier = bundleIdentifier
        expectedInstalledBuild = installedBuild
    }

    static func decode(_ data: Data, root: String?) throws -> Self {
        let result = withBytes(data) { request in
            withBytes(Data((root ?? "").utf8)) { helm_external_adoption_request(request, $0) }
        }
        guard result == UInt32(HELM_EXTERNAL_UNRESOLVED) else { throw BootstrapFailure.invalidMessage }
        return try JSONDecoder().decode(Self.self, from: data)
    }
}

public struct ExternalAdoptionReview {
    public let request: ExternalAdoptionRequest
    let handle: Data
}

public enum ExternalAdoptionOutcome: String {
    case recorded, reviewChanged, outcomeUnknown

    init?(code: UInt32) {
        switch code {
        case 51: self = .recorded
        case 52: self = .reviewChanged
        case 53: self = .outcomeUnknown
        default: return nil
        }
    }

    var code: UInt32 {
        switch self {
        case .recorded: return 51
        case .reviewChanged: return 52
        case .outcomeUnknown: return 53
        }
    }
}

/// Worker-confined and consumed even when admission or precommit checks fail.
final class PreparedAdoption {
    private var operation: ((_ admit: () throws -> Void) throws -> ExternalAdoptionOutcome)?
    init(_ operation: @escaping (_ admit: () throws -> Void) throws -> ExternalAdoptionOutcome) {
        self.operation = operation
    }
    func confirm(_ admit: () throws -> Void) throws -> ExternalAdoptionOutcome {
        guard let operation else { throw BootstrapFailure.invalidMessage }
        self.operation = nil
        return try operation(admit)
    }
}

/// Trusted local inputs only, never Codable. No shipping observer currently
/// supplies complete boundary/ownership proof; production adoption is disabled.
struct NativeAdoptionObservation: Equatable {
    let target: NativeTargetEvidence
    let boundary: NativeAdoptionBoundary
    let ownershipComplete: Bool
}

struct NativeAdoptionProcessor {
    let observe: (String) throws -> NativeAdoptionObservation
    var userApplications: () -> String? = { NativeApplicationRoots.userApplications }
    var scope: (String) -> PrivateLedgerDirectory = {
        PrivateLedgerDirectory(home: URL(fileURLWithPath: $0).deletingLastPathComponent())
    }
    var seconds: () -> UInt64 = { DispatchTime.now().uptimeNanoseconds / 1_000_000_000 }

    func prepare(_ data: Data) throws -> PreparedAdoption {
        let root = userApplications()
        let request = try ExternalAdoptionRequest.decode(data, root: root)
        guard let root else { throw BootstrapFailure.invalidated }
        let snapshot = try observe(request.targetPath)
        guard snapshot.ownershipComplete else { throw BootstrapFailure.invalidated }
        let scope = scope(root)
        let recheck = { () throws -> NativeAdoptionObservation in
            guard userApplications() == root else { throw BootstrapFailure.invalidated }
            let current = try observe(request.targetPath)
            guard current == snapshot,
                  userApplications() == root else { throw BootstrapFailure.invalidated }
            return current
        }
        let review = try scope.withDatabase(createIfMissing: false) { path, _ in
            let current = try recheck()
            return try NativeAdoptionReview(path: path, request: data, target: current.target,
                boundary: current.boundary, userApplications: root, now: seconds())
        }
        _ = try recheck()
        return PreparedAdoption { admit in
            var admitted = false
            do {
                return try scope.withDatabase(createIfMissing: false) { path, _ in
                    // Fresh complete snapshots must agree with the reviewed one;
                    // native collection never blocks the connection's gate queue.
                    let current = try recheck()
                    try admit()
                    admitted = true
                    let outcome = review.confirm(path: path, target: current.target,
                        boundary: current.boundary, userApplications: root, now: seconds())
                    _ = try recheck()
                    switch outcome {
                    case .recorded: return .recorded
                    case .reviewChanged: return .reviewChanged
                    case .outcomeUnknown: return .outcomeUnknown
                    }
                }
            } catch {
                if admitted { return .outcomeUnknown }
                throw error
            }
        }
    }
}

/// One pending adoption per connection, confined to that connection's worker.
final class AdoptionCoordinator {
    private let prepare: (Data) throws -> PreparedAdoption
    private var pending: (handle: Data, review: PreparedAdoption)?
    init(prepare: @escaping (Data) throws -> PreparedAdoption) { self.prepare = prepare }

    func review(_ data: Data) throws -> Data {
        pending = nil
        _ = try ExternalAdoptionRequest.decode(data, root: NativeApplicationRoots.userApplications)
        let review = try prepare(data)
        let handle = try BootstrapWire.nonce()
        pending = (handle, review)
        return handle
    }

    func confirm(_ handle: Data, admit: () throws -> Void) throws -> ExternalAdoptionOutcome {
        let pending = pending
        self.pending = nil
        guard handle.count == BootstrapWire.bytes, let pending, pending.handle == handle else {
            throw BootstrapFailure.invalidMessage
        }
        return try pending.review.confirm(admit)
    }

    func clear() { pending = nil }
}
