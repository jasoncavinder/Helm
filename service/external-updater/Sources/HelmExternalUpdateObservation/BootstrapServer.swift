import Darwin
import Foundation

/// Per-connection diagnostics and denial-only revocation. No adoption grant,
/// candidate download or installer operation is exposed. Consent inspection uses
/// only existing helper storage; ordinary preflight remains database-free.
public final class ExternalUpdaterBootstrapServer: NSObject, ExternalUpdaterBootstrapProtocol {
    private let connection: NSXPCConnection
    private let queue = DispatchQueue(label: "com.jasoncavinder.Helm.external-bootstrap-server")
    private let notifications = DispatchQueue(label: "com.jasoncavinder.Helm.external-bootstrap-server-events")
    private let event: (BootstrapEvent) -> Void
    private let worker = DispatchQueue(label: "com.jasoncavinder.Helm.external-preflight-worker")
    private let assess: ((Data) throws -> UInt32)?
    private let consent: ((Data) throws -> UInt32)?
    private let revocations: RevocationCoordinator?
    private let started = DispatchTime.now().uptimeNanoseconds
    private var operations = PreflightGate(started: DispatchTime.now().uptimeNanoseconds)
    private var pendingReply: ((UInt64, UInt32, Data?) -> Void)?
    private var pendingSequence: UInt64 = 0
    private var established = false
    private var closed = false

    /// `connection` must be newly accepted and inactive. Do not call admit first:
    /// Foundation permits configuring a connection requirement only once.
    public convenience init(connection: NSXPCConnection, helperIdentity: NativeHelperEvidence? = nil,
                            event: @escaping (BootstrapEvent) -> Void) throws {
        let authentication = try ExternalUpdaterPeerAuthentication()
        guard authentication.admit(connection) else { throw BootstrapFailure.invalidAccount }
        let processor = helperIdentity.map { NativePreflightProcessor(identity: $0) }
        self.init(configuredConnection: connection, assess: processor.map { processor in
            { try processor.assess($0).code }
        }, consent: helperIdentity.map { identity in
            { try NativeConsentProcessor(identity: identity).assess($0).code }
        }, revocation: helperIdentity.map { identity in
            { try NativeRevocationProcessor(identity: identity).prepare($0) }
        }, event: event)
    }

    private init(configuredConnection: NSXPCConnection, assess: ((Data) throws -> UInt32)?, consent: ((Data) throws -> UInt32)?,
                 revocation: ((Data) throws -> PreparedRevocation)?,
                 event: @escaping (BootstrapEvent) -> Void) {
        connection = configuredConnection
        self.event = event
        self.assess = assess
        self.consent = consent
        revocations = revocation.map { RevocationCoordinator(prepare: $0) }
        super.init()
        connection.exportedInterface = NSXPCInterface(with: ExternalUpdaterBootstrapProtocol.self)
        connection.exportedObject = self
        connection.interruptionHandler = { [weak self] in self?.close(.interrupted) }
        connection.invalidationHandler = { [weak self] in self?.close(.invalidated) }
        queue.asyncAfter(deadline: .now() + .seconds(5)) { [weak self] in
            guard let self, !self.established else { return }
            self.finish(.expired)
        }
        queue.asyncAfter(deadline: .now() + .seconds(120)) { [weak self] in self?.finish(.expired) }
        connection.activate()
    }

    #if DEBUG
    convenience init(testingConnection: NSXPCConnection,
                     assess: ((Data) throws -> NativePolicyAssessment)? = nil,
                     consent: ((Data) throws -> ExternalConsentStatus)? = nil,
                     revocation: ((Data) throws -> PreparedRevocation)? = nil,
                     event: @escaping (BootstrapEvent) -> Void) {
        self.init(configuredConnection: testingConnection,
                  assess: assess.map { call in { try call($0).code } },
                  consent: consent.map { call in { try call($0).code } }, revocation: revocation, event: event)
    }
    #endif

    public func hello(version: UInt32, challenge: Data, reply: @escaping (UInt32, Data?, Data?) -> Void) {
        queue.async { [weak self] in
            guard let self else { return }
            let now = DispatchTime.now().uptimeNanoseconds
            guard !self.closed, !self.established, version == BootstrapWire.version,
                  challenge.count == BootstrapWire.bytes else {
                reply(0, nil, nil)
                self.finish(.invalidMessage)
                return
            }
            guard now >= self.started, now - self.started < BootstrapWire.handshakeNanoseconds else {
                reply(0, nil, nil)
                self.finish(.expired)
                return
            }
            do {
                let session = try BootstrapWire.nonce()
                self.operations.establish(session)
                self.established = true
                reply(BootstrapWire.version, challenge, session)
                self.emit(.ready)
            } catch {
                reply(0, nil, nil)
                self.finish(.entropyUnavailable)
            }
        }
    }

    public func cancel() { close(.cancelled) }

    public func preflight(session: Data, sequence: UInt64, request: Data,
                          reply: @escaping (UInt64, UInt32) -> Void) {
        perform(assess: assess.map { call in { BootstrapResponse(code: try call($0)) } },
                session: session, sequence: sequence, request: request) { echo, code, _ in reply(echo, code) }
    }

    public func consentStatus(session: Data, sequence: UInt64, request: Data,
                              reply: @escaping (UInt64, UInt32) -> Void) {
        perform(assess: consent.map { call in { BootstrapResponse(code: try call($0)) } },
                session: session, sequence: sequence, request: request) { echo, code, _ in reply(echo, code) }
    }

    public func reviewRevocation(session: Data, sequence: UInt64, request: Data,
                                 reply: @escaping (UInt64, UInt32, Data?) -> Void) {
        perform(assess: revocations.map { controller in { BootstrapResponse(code: 39, payload: try controller.review($0)) } },
                session: session, sequence: sequence, request: request, reply: reply)
    }

    public func confirmRevocation(session: Data, sequence: UInt64, review: Data,
                                  reply: @escaping (UInt64, UInt32) -> Void) {
        perform(assess: { [weak self] handle in
            guard let self, let controller = self.revocations else { throw BootstrapFailure.invalidMessage }
            let result = try controller.confirm(handle) {
                try self.queue.sync {
                    try self.operations.admitRevocation(sequence: sequence, now: DispatchTime.now().uptimeNanoseconds)
                }
            }
            return BootstrapResponse(code: result.code)
        }, session: session, sequence: sequence, request: review) { echo, code, _ in reply(echo, code) }
    }

    private func perform(assess: ((Data) throws -> BootstrapResponse)?, session: Data, sequence: UInt64, request: Data,
                         reply: @escaping (UInt64, UInt32, Data?) -> Void) {
        queue.async { [weak self] in
            guard let self else { reply(sequence, 0, nil); return }
            do {
                guard !self.closed, self.established, let assess,
                      self.connection.effectiveUserIdentifier == geteuid() else {
                    throw BootstrapFailure.invalidMessage
                }
                try self.operations.begin(token: session, sequence: sequence, bytes: request.count,
                                          now: DispatchTime.now().uptimeNanoseconds)
                self.pendingReply = reply
                self.pendingSequence = sequence
                // Native signing/filesystem inspection must not block the queue
                // handling expiry or loss. Late work can never publish a result.
                self.worker.async { [weak self] in
                    let result = Result { try assess(request) }
                    self?.queue.async { [weak self] in self?.complete(sequence: sequence, result: result) }
                }
                self.queue.asyncAfter(deadline: .now() + .seconds(15)) { [weak self] in
                    guard let self, self.operations.isPending(sequence) else { return }
                    self.finish(.expired)
                }
            } catch {
                reply(sequence, 0, nil)
                self.finish((error as? BootstrapFailure) ?? .invalidMessage)
            }
        }
    }

    private func complete(sequence: UInt64, result: Result<BootstrapResponse, Error>) {
        guard !closed else { return }
        guard operations.complete(sequence: sequence, now: DispatchTime.now().uptimeNanoseconds) else {
            finish(.expired)
            return
        }
        switch result {
        case .success(let assessment):
            let reply = pendingReply
            pendingReply = nil
            reply?(sequence, assessment.code, assessment.payload)
        case .failure(let error):
            // Fixed error categories only: never log paths, handles or ledger data.
            let reason: String
            if let failure = error as? BootstrapFailure {
                reason = failure.rawValue
            } else if let failure = error as? HelperLedgerFailure {
                reason = "ledger.\(failure)"
            } else if let failure = error as? HelperObservationFailure {
                reason = "helper.\(failure.rawValue)"
            } else {
                reason = "observationUnavailable"
            }
            FileHandle.standardError.write(Data("External updater request rejected: \(reason)\n".utf8))
            finish((error as? BootstrapFailure) ?? .invalidated)
        }
    }

    private func close(_ failure: BootstrapFailure) {
        queue.async { [weak self] in self?.finish(failure) }
    }

    private func finish(_ failure: BootstrapFailure) {
        guard !closed else { return }
        closed = true
        operations.close()
        let reply = pendingReply
        pendingReply = nil
        reply?(pendingSequence, 0, nil)
        if let revocations { worker.async { revocations.clear() } }
        connection.invalidate()
        connection.exportedObject = nil
        emit(.closed(failure))
    }

    private func emit(_ value: BootstrapEvent) {
        notifications.async { [event] in event(value) }
    }
}
