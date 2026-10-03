import Foundation
import Security

/// The first message contains no target path, update metadata, or operation.
/// Foundation authenticates the helper's reply before the client accepts it.
@objc(HELMExternalUpdaterBootstrapProtocol)
public protocol ExternalUpdaterBootstrapProtocol {
    func hello(version: UInt32, challenge: Data,
               reply: @escaping (UInt32, Data?, Data?) -> Void)
    func preflight(session: Data, sequence: UInt64, request: Data,
                   reply: @escaping (UInt64, UInt32) -> Void)
}

public enum BootstrapFailure: String, Error {
    case entropyUnavailable
    case invalidMessage
    case invalidAccount
    case expired
    case interrupted
    case invalidated
    case cancelled
    case transport
}

/// Ready means this bounded transport handshake completed, not that an app or
/// update is authorized. Only read-only preflight may follow readiness.
public enum BootstrapEvent {
    case ready
    case closed(BootstrapFailure)
}

enum BootstrapWire {
    static let version: UInt32 = 1
    static let bytes = 32
    static let handshakeNanoseconds: UInt64 = 5_000_000_000
    static let lifetimeNanoseconds: UInt64 = 120_000_000_000

    static func nonce() throws -> Data {
        var bytes = [UInt8](repeating: 0, count: Self.bytes)
        guard SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes) == errSecSuccess else {
            throw BootstrapFailure.entropyUnavailable
        }
        return Data(bytes)
    }
}

struct BootstrapReplyGate {
    let challenge: Data
    let account: uid_t
    let started: UInt64
    private(set) var established = false
    private(set) var closed = false

    mutating func accept(version: UInt32, echo: Data?, session: Data?,
                         peerAccount: uid_t, now: UInt64) throws {
        guard !closed, !established else { throw BootstrapFailure.invalidMessage }
        guard now >= started, now - started < BootstrapWire.handshakeNanoseconds else {
            throw BootstrapFailure.expired
        }
        guard account != 0, peerAccount == account else { throw BootstrapFailure.invalidAccount }
        guard version == BootstrapWire.version, echo == challenge,
              challenge.count == BootstrapWire.bytes, session?.count == BootstrapWire.bytes else {
            throw BootstrapFailure.invalidMessage
        }
        established = true
    }

    mutating func close() -> Bool {
        guard !closed else { return false }
        closed = true
        return true
    }
}
