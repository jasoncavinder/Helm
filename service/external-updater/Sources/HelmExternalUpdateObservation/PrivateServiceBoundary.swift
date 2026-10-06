import Darwin
import Foundation

/// Connection-local evidence, never serialized or persisted. Production creation
/// requires the owned service listener plus a configured native peer requirement.
final class NativePrivateServiceBoundary {
    private enum State { case accepted, established, closed }
    private let lock = NSLock()
    private var state = State.accepted
    private let identity: NativeHelperEvidence
    private let currentMessage: () -> Bool
    private let peerAccount: () -> uid_t
    private let accounts: () -> (uid_t, uid_t)
    private let helper: () throws -> NativeHelperEvidence
    private let clock: () -> UInt64
    private let started: UInt64

    convenience init(acceptance: PrivateServiceAcceptance) {
        let connection = acceptance.connection
        self.init(identity: acceptance.identity,
                  currentMessage: { NSXPCConnection.current() === connection },
                  peerAccount: { connection.effectiveUserIdentifier }, accounts: { (getuid(), geteuid()) },
                  helper: { try NativeHelperObserver().observeSelf() }, clock: { DispatchTime.now().uptimeNanoseconds })
    }

    private init(identity: NativeHelperEvidence, currentMessage: @escaping () -> Bool,
                 peerAccount: @escaping () -> uid_t, accounts: @escaping () -> (uid_t, uid_t),
                 helper: @escaping () throws -> NativeHelperEvidence, clock: @escaping () -> UInt64) {
        self.identity = identity
        self.currentMessage = currentMessage
        self.peerAccount = peerAccount
        self.accounts = accounts
        self.helper = helper
        self.clock = clock
        started = clock()
    }

    #if DEBUG
    convenience init(testingIdentity: NativeHelperEvidence, currentMessage: @escaping () -> Bool = { true },
                     peerAccount: @escaping () -> uid_t = { geteuid() },
                     accounts: @escaping () -> (uid_t, uid_t) = { (getuid(), geteuid()) },
                     helper: @escaping () throws -> NativeHelperEvidence,
                     clock: @escaping () -> UInt64 = { DispatchTime.now().uptimeNanoseconds }) {
        self.init(identity: testingIdentity, currentMessage: currentMessage, peerAccount: peerAccount,
                  accounts: accounts, helper: helper, clock: clock)
    }
    #endif

    // Must run on Foundation's exported-method invocation, before dispatching
    // to the server queue. A locally called method cannot mint peer evidence.
    func deliveredMessage() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return state != .closed && currentMessage() && accountMatches()
    }

    func establish() throws {
        lock.lock()
        defer { lock.unlock() }
        let now = clock()
        guard state == .accepted, accountMatches(), now >= started,
              now - started < BootstrapWire.handshakeNanoseconds else { throw BootstrapFailure.expired }
        state = .established
    }

    func close() {
        lock.lock()
        state = .closed
        lock.unlock()
    }

    /// Called only by the server's admitted worker. The existing request gate
    /// still enforces sequence, deadlines, cancellation and commit admission.
    func withObservation<T>(_ operation: (NativeAdoptionBoundary) throws -> T) throws -> T {
        try validateLive()
        let current = try helper()
        guard current == identity else { throw HelperObservationFailure.changedDuringObservation }
        try HelperBundleFormat.privateService.validate(path: URL(fileURLWithPath: current.canonicalPath))
        try validateLive()
        let boundary = NativeAdoptionBoundary(helperIdentifier: current.bundleIdentifier,
            helperTeamIdentifier: current.teamIdentifier, helperCodeDirectoryHash: Data(current.codeDirectoryHash),
            callerIdentifier: "com.jasoncavinder.Helm", callerTeamIdentifier: "V73WPJR9M4",
            authenticatedLiveCaller: true, developerIDSignatureValid: true, notarizationAccepted: true,
            helmSandboxPreserved: true, externalHelperUnsandboxed: true, directConsumerChannel: true)
        let result = try operation(boundary)
        guard try helper() == current else { throw HelperObservationFailure.changedDuringObservation }
        try validateLive()
        return result
    }

    private func validateLive() throws {
        lock.lock()
        defer { lock.unlock() }
        let now = clock()
        guard state == .established, accountMatches(), now >= started,
              now - started < BootstrapWire.lifetimeNanoseconds else { throw BootstrapFailure.invalidated }
    }

    private func accountMatches() -> Bool {
        let (real, effective) = accounts()
        return real != 0 && real == effective && effective == identity.account && peerAccount() == effective
    }
}
