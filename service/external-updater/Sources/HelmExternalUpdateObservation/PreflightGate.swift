import Foundation

/// Queue-confined transport state, not durable permission or a target reservation.
struct PreflightGate {
    private var token: Data?
    private var next: UInt64 = 1
    private var pending: UInt64?
    private var pendingAt: UInt64?
    private var closed = false
    let started: UInt64
    static let maximumRequests: UInt64 = 8
    static let maximumBytes = 8192
    static let requestNanoseconds: UInt64 = 15_000_000_000

    init(started: UInt64) { self.started = started }

    mutating func establish(_ token: Data) {
        guard !closed, self.token == nil, token.count == BootstrapWire.bytes else { return }
        self.token = token
    }

    mutating func begin(token: Data, sequence: UInt64, bytes: Int, now: UInt64) throws {
        guard !closed, let expected = self.token, token == expected,
              token.count == BootstrapWire.bytes, sequence == next,
              next <= Self.maximumRequests, pending == nil,
              bytes > 0, bytes <= Self.maximumBytes else { throw BootstrapFailure.invalidMessage }
        guard live(now) else { throw BootstrapFailure.expired }
        pending = sequence
        pendingAt = now
        next += 1
    }

    mutating func complete(sequence: UInt64, now: UInt64) -> Bool {
        guard !closed, pending == sequence, live(now), let pendingAt,
              now >= pendingAt, now - pendingAt < Self.requestNanoseconds else { return false }
        pending = nil
        self.pendingAt = nil
        return true
    }

    func isPending(_ sequence: UInt64) -> Bool { !closed && pending == sequence }

    mutating func close() { closed = true; token = nil; pending = nil; pendingAt = nil }

    private func live(_ now: UInt64) -> Bool {
        now >= started && now - started < BootstrapWire.lifetimeNanoseconds
    }
}
