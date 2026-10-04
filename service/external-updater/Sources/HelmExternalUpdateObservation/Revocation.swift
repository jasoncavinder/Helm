import CExternalUpdatePolicy
import Foundation

public struct ExternalRevocationRequest: Codable {
    public let schemaVersion: UInt32
    public let requestId: String
    public let targetPath: String

    public init(targetPath: String) {
        schemaVersion = 1
        requestId = UUID().uuidString.lowercased()
        self.targetPath = targetPath
    }

    static func decode(_ data: Data, root: String?) throws -> Self {
        let valid = withBytes(data) { request in
            withBytes(Data((root ?? "").utf8)) { helm_external_revocation_request(request, $0) }
        }
        guard valid == UInt32(HELM_EXTERNAL_UNRESOLVED) else { throw BootstrapFailure.invalidMessage }
        return try JSONDecoder().decode(Self.self, from: data)
    }
}

public struct ExternalRevocationReview {
    public let targetPath: String
    let handle: Data
}

public enum ExternalRevocationOutcome: String {
    case revoked, reviewChanged, outcomeUnknown
    init?(code: UInt32) {
        switch code {
        case 40: self = .revoked
        case 41: self = .reviewChanged
        case 42: self = .outcomeUnknown
        default: return nil
        }
    }
    var code: UInt32 {
        switch self {
        case .revoked: return 40
        case .reviewChanged: return 41
        case .outcomeUnknown: return 42
        }
    }
}

func withBytes<T>(_ data: Data, _ operation: (HelmExternalBytes) throws -> T) rethrows -> T {
    try data.withUnsafeBytes { try operation(HelmExternalBytes(data: $0.bindMemory(to: UInt8.self).baseAddress, length: $0.count)) }
}

/// Worker-confined. Only this process owns the Rust review, never its XPC peer.
final class NativeRevocationReview {
    private var handle: OpaquePointer?
    init(path: String, request: Data, root: String?, now: UInt64) throws {
        handle = withBytes(Data(path.utf8)) { path in
            withBytes(request) { request in
                withBytes(Data((root ?? "").utf8)) { helm_external_revocation_prepare(path, request, $0, now) }
            }
        }
        guard handle != nil else { throw HelperLedgerFailure.storageRejected }
    }
    deinit { helm_external_revocation_free(handle) }
    func confirm(path: String, now: UInt64) -> ExternalRevocationOutcome {
        guard let handle else { return .reviewChanged }
        self.handle = nil
        return withBytes(Data(path.utf8)) {
            ExternalRevocationOutcome(code: helm_external_revocation_confirm(handle, $0, now)) ?? .outcomeUnknown
        }
    }
}

struct PreparedRevocation {
    let confirm: (_ admit: () throws -> Void) throws -> ExternalRevocationOutcome
}

struct NativeRevocationProcessor {
    let identity: NativeHelperEvidence

    func prepare(_ request: Data) throws -> PreparedRevocation {
        let root = NativeApplicationRoots.userApplications
        _ = try ExternalRevocationRequest.decode(request, root: root)
        guard try NativeHelperObserver().observeSelf() == identity, let root else { throw BootstrapFailure.invalidated }
        let scope = PrivateLedgerDirectory(home: URL(fileURLWithPath: root).deletingLastPathComponent())
        let review = try scope.withDatabase(createIfMissing: false) { path, _ in
            try NativeRevocationReview(path: path, request: request, root: root, now: Self.seconds)
        }
        guard try NativeHelperObserver().observeSelf() == identity,
              NativeApplicationRoots.userApplications == root else { throw BootstrapFailure.invalidated }
        return PreparedRevocation { admit in
            guard try NativeHelperObserver().observeSelf() == identity,
                  NativeApplicationRoots.userApplications == root else { throw BootstrapFailure.invalidated }
            var admitted = false
            do {
                return try scope.withDatabase(createIfMissing: false) { path, _ in
                    guard try NativeHelperObserver().observeSelf() == identity,
                          NativeApplicationRoots.userApplications == root else { throw BootstrapFailure.invalidated }
                    try admit()
                    admitted = true
                    let outcome = review.confirm(path: path, now: Self.seconds)
                    guard try NativeHelperObserver().observeSelf() == identity,
                          NativeApplicationRoots.userApplications == root else { throw BootstrapFailure.invalidated }
                    return outcome
                }
            } catch {
                // A post-commit lease/identity failure cannot mean "nothing happened".
                if admitted { return .outcomeUnknown }
                throw error
            }
        }
    }
    private static var seconds: UInt64 { DispatchTime.now().uptimeNanoseconds / 1_000_000_000 }
}

/// One pending review per authenticated connection; never shared across clients.
final class RevocationCoordinator {
    private let prepare: (Data) throws -> PreparedRevocation
    private var pending: (handle: Data, review: PreparedRevocation)?
    init(prepare: @escaping (Data) throws -> PreparedRevocation) { self.prepare = prepare }
    func review(_ request: Data) throws -> Data {
        pending = nil
        let review = try prepare(request)
        let handle = try BootstrapWire.nonce()
        pending = (handle, review)
        return handle
    }
    func confirm(_ handle: Data, admit: () throws -> Void) throws -> ExternalRevocationOutcome {
        let review = pending
        pending = nil
        guard handle.count == BootstrapWire.bytes, let review, review.handle == handle else { throw BootstrapFailure.invalidMessage }
        return try review.review.confirm(admit)
    }
    func clear() { pending = nil }
}
