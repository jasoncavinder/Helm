import Darwin
import Foundation

/// Per-connection handshake and bounded read-only preflight. No adoption grant,
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
    private let started = DispatchTime.now().uptimeNanoseconds
    private var operations = PreflightGate(started: DispatchTime.now().uptimeNanoseconds)
    private var pendingReply: ((UInt64, UInt32) -> Void)?
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
        }, event: event)
    }

    private init(configuredConnection: NSXPCConnection, assess: ((Data) throws -> UInt32)?, consent: ((Data) throws -> UInt32)?,
                 event: @escaping (BootstrapEvent) -> Void) {
        connection = configuredConnection
        self.event = event
        self.assess = assess
        self.consent = consent
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
                     event: @escaping (BootstrapEvent) -> Void) {
        self.init(configuredConnection: testingConnection,
                  assess: assess.map { call in { try call($0).code } },
                  consent: consent.map { call in { try call($0).code } }, event: event)
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
        perform(assess: assess, session: session, sequence: sequence, request: request, reply: reply)
    }

    public func consentStatus(session: Data, sequence: UInt64, request: Data,
                              reply: @escaping (UInt64, UInt32) -> Void) {
        perform(assess: consent, session: session, sequence: sequence, request: request, reply: reply)
    }

    private func perform(assess: ((Data) throws -> UInt32)?, session: Data, sequence: UInt64, request: Data,
                         reply: @escaping (UInt64, UInt32) -> Void) {
        queue.async { [weak self] in
            guard let self else { reply(sequence, 0); return }
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
                reply(sequence, 0)
                self.finish((error as? BootstrapFailure) ?? .invalidMessage)
            }
        }
    }

    private func complete(sequence: UInt64, result: Result<UInt32, Error>) {
        guard !closed else { return }
        guard operations.complete(sequence: sequence, now: DispatchTime.now().uptimeNanoseconds) else {
            finish(.expired)
            return
        }
        switch result {
        case .success(let assessment):
            let reply = pendingReply
            pendingReply = nil
            reply?(sequence, assessment)
        case .failure(let error): finish((error as? BootstrapFailure) ?? .invalidated)
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
        reply?(pendingSequence, 0)
        connection.invalidate()
        connection.exportedObject = nil
        emit(.closed(failure))
    }

    private func emit(_ value: BootstrapEvent) {
        notifications.async { [event] in event(value) }
    }
}
