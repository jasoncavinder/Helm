import Darwin
import Foundation

public final class ExternalUpdaterBootstrapClient {
    private let connection: NSXPCConnection
    private let queue = DispatchQueue(label: "com.jasoncavinder.Helm.external-bootstrap-client")
    private let notifications = DispatchQueue(label: "com.jasoncavinder.Helm.external-bootstrap-client-events")
    private let event: (BootstrapEvent) -> Void
    private var gate: BootstrapReplyGate
    private var started = false
    private var session: Data?
    private var nextSequence: UInt64 = 1
    private enum Operation { case preflight, consent, reviewRevocation, confirmRevocation, reviewAdoption, confirmAdoption }
    private var pending: (sequence: UInt64, started: UInt64, operation: Operation, reply: (Result<BootstrapResponse, BootstrapFailure>) -> Void)?

    /// Retain until finished; release/cancel closes the connection. The endpoint
    /// alone is untrusted. Production always installs the fixed helper requirement.
    public convenience init(endpoint: NSXPCListenerEndpoint, event: @escaping (BootstrapEvent) -> Void) throws {
        let authentication = try ExternalUpdaterPeerAuthentication()
        try self.init(connection: authentication.makeConnection(to: endpoint), event: event)
    }

    /// The named service is still untrusted until the data-free handshake and
    /// native peer/account validation complete. App requests wait for readiness.
    public convenience init(event: @escaping (BootstrapEvent) -> Void) throws {
        let authentication = try ExternalUpdaterPeerAuthentication()
        try self.init(connection: authentication.makeBootstrapServiceConnection(), event: event)
    }

    /// Staged private service route. The shipping app does not embed this service
    /// yet. The route alone proves neither an unsandboxed peer nor authorization.
    public static func bundledService(event: @escaping (BootstrapEvent) -> Void) throws -> ExternalUpdaterBootstrapClient {
        let authentication = try ExternalUpdaterPeerAuthentication()
        return try ExternalUpdaterBootstrapClient(connection: authentication.makeBundledServiceConnection(), event: event)
    }

    private init(connection: NSXPCConnection, event: @escaping (BootstrapEvent) -> Void) throws {
        self.connection = connection
        self.event = event
        gate = BootstrapReplyGate(challenge: try BootstrapWire.nonce(), account: geteuid(),
                                  started: DispatchTime.now().uptimeNanoseconds)
        connection.remoteObjectInterface = NSXPCInterface(with: ExternalUpdaterBootstrapProtocol.self)
        connection.interruptionHandler = { [weak self] in self?.close(.interrupted) }
        connection.invalidationHandler = { [weak self] in self?.close(.invalidated) }
    }

    #if DEBUG
    // Test-only unrestricted transport isolates protocol behavior from identity
    // rejection. Release builds have no alternate connection constructor.
    convenience init(testingConnection: NSXPCConnection, event: @escaping (BootstrapEvent) -> Void) throws {
        try self.init(connection: testingConnection, event: event)
    }
    #endif

    deinit { connection.invalidate() }

    public func begin() {
        queue.async { [weak self] in
            guard let self, !self.started, !self.gate.closed else { return }
            self.started = true
            self.gate = BootstrapReplyGate(challenge: self.gate.challenge, account: geteuid(),
                                           started: DispatchTime.now().uptimeNanoseconds)
            self.connection.activate()
            self.queue.asyncAfter(deadline: .now() + .seconds(5)) { [weak self] in
                guard let self, !self.gate.established else { return }
                self.finish(.expired)
            }
            self.queue.asyncAfter(deadline: .now() + .seconds(120)) { [weak self] in self?.finish(.expired) }
            guard let proxy = self.connection.remoteObjectProxyWithErrorHandler({ [weak self] _ in
                self?.close(.transport)
            }) as? ExternalUpdaterBootstrapProtocol else {
                self.finish(.transport)
                return
            }
            proxy.hello(version: BootstrapWire.version, challenge: self.gate.challenge) { [weak self] version, echo, session in
                self?.receive(version: version, echo: echo, session: session)
            }
        }
    }

    public func cancel() { close(.cancelled) }

    private func receive(version: UInt32, echo: Data?, session: Data?) {
        queue.async { [weak self] in
            guard let self, !self.gate.closed else { return }
            do {
                try self.gate.accept(version: version, echo: echo, session: session,
                                     peerAccount: self.connection.effectiveUserIdentifier,
                                     now: DispatchTime.now().uptimeNanoseconds)
                self.session = session
                self.emit(.ready)
            } catch let error as BootstrapFailure {
                self.finish(error)
            } catch {
                self.finish(.invalidMessage)
            }
        }
    }

    private func close(_ failure: BootstrapFailure) {
        queue.async { [weak self] in self?.finish(failure) }
    }

    /// Diagnostic only; an unresolved response never permits adoption or updates.
    /// No app data is sent until the helper has authenticated its hello reply.
    public func preflight(_ request: ExternalPreflightRequest,
                          reply: @escaping (Result<NativePolicyAssessment, BootstrapFailure>) -> Void) {
        encode(request, operation: .preflight) { reply($0.map { NativePolicyAssessment(code: $0.code) }) }
    }

    public func consentStatus(_ request: ExternalPreflightRequest,
                              reply: @escaping (Result<ExternalConsentStatus, BootstrapFailure>) -> Void) {
        encode(request, operation: .consent) { result in
            reply(result.flatMap { code in
                ExternalConsentStatus(code: code.code).map { .success($0) } ?? .failure(.invalidMessage)
            })
        }
    }

    public func reviewRevocation(_ request: ExternalRevocationRequest,
                                 reply: @escaping (Result<ExternalRevocationReview, BootstrapFailure>) -> Void) {
        encode(request, operation: .reviewRevocation) { result in
            reply(result.flatMap { response in
                guard let handle = response.payload else { return .failure(.invalidMessage) }
                return .success(ExternalRevocationReview(targetPath: request.targetPath, handle: handle))
            })
        }
    }

    /// Transport failure after confirmation can mean the write committed. Never
    /// automatically retry; inspect/review current history before a new decision.
    public func confirmRevocation(_ review: ExternalRevocationReview,
                                  reply: @escaping (Result<ExternalRevocationOutcome, BootstrapFailure>) -> Void) {
        send(review.handle, operation: .confirmRevocation) { result in
            reply(result.flatMap { ExternalRevocationOutcome(code: $0.code).map { .success($0) } ?? .failure(.invalidMessage) })
        }
    }

    /// Staged transport only: shipping helper construction rejects adoption until
    /// native boundary and ownership proof are connected. Never auto-confirm.
    public func reviewAdoption(_ request: ExternalAdoptionRequest,
                               reply: @escaping (Result<ExternalAdoptionReview, BootstrapFailure>) -> Void) {
        encode(request, operation: .reviewAdoption) { result in
            reply(result.flatMap { response in
                guard let handle = response.payload else { return .failure(.invalidMessage) }
                return .success(ExternalAdoptionReview(request: request, handle: handle))
            })
        }
    }

    /// Failure/lost reply may hide committed consent. Inspect current history
    /// and obtain a new explicit review; this client never retries confirmation.
    public func confirmAdoption(_ review: ExternalAdoptionReview,
                                reply: @escaping (Result<ExternalAdoptionOutcome, BootstrapFailure>) -> Void) {
        send(review.handle, operation: .confirmAdoption) { result in
            reply(result.flatMap { ExternalAdoptionOutcome(code: $0.code).map { .success($0) } ?? .failure(.invalidMessage) })
        }
    }

    private func encode<T: Encodable>(_ request: T, operation: Operation,
                                      reply: @escaping (Result<BootstrapResponse, BootstrapFailure>) -> Void) {
        do { send(try JSONEncoder().encode(request), operation: operation, reply: reply) } catch {
            notifications.async { reply(.failure(.invalidMessage)) }
        }
    }

    private func send(_ data: Data, operation: Operation,
                      reply: @escaping (Result<BootstrapResponse, BootstrapFailure>) -> Void) {
        queue.async { [weak self] in
            guard let self else { reply(.failure(.invalidated)); return }
            guard self.gate.established, !self.gate.closed, let session = self.session,
                  self.pending == nil, self.nextSequence <= PreflightGate.maximumRequests else {
                self.notifications.async { reply(.failure(.invalidMessage)) }
                return
            }
            let now = DispatchTime.now().uptimeNanoseconds
            guard now >= self.gate.started, now - self.gate.started < BootstrapWire.lifetimeNanoseconds else {
                self.notifications.async { reply(.failure(.expired)) }
                self.finish(.expired)
                return
            }
            do {
                switch operation {
                case .preflight, .consent:
                    _ = try ExternalPreflightRequest.decode(data, userApplications: NativeApplicationRoots.userApplications)
                case .reviewRevocation:
                    _ = try ExternalRevocationRequest.decode(data, root: NativeApplicationRoots.userApplications)
                case .reviewAdoption:
                    _ = try ExternalAdoptionRequest.decode(data, root: NativeApplicationRoots.userApplications)
                case .confirmRevocation, .confirmAdoption:
                    guard data.count == BootstrapWire.bytes else { throw BootstrapFailure.invalidMessage }
                }
                let sequence = self.nextSequence
                self.nextSequence += 1
                self.pending = (sequence, DispatchTime.now().uptimeNanoseconds, operation, reply)
                guard let proxy = self.connection.remoteObjectProxyWithErrorHandler({ [weak self] _ in
                    self?.close(.transport)
                }) as? ExternalUpdaterBootstrapProtocol else {
                    self.finish(.transport)
                    return
                }
                let completed: (UInt64, UInt32) -> Void = { [weak self] echo, code in
                    self?.receiveResponse(sequence: sequence, echo: echo, code: code, payload: nil)
                }
                switch operation {
                case .consent:
                    proxy.consentStatus(session: session, sequence: sequence, request: data, reply: completed)
                case .preflight:
                    proxy.preflight(session: session, sequence: sequence, request: data, reply: completed)
                case .reviewRevocation:
                    proxy.reviewRevocation(session: session, sequence: sequence, request: data) { [weak self] echo, code, payload in
                        self?.receiveResponse(sequence: sequence, echo: echo, code: code, payload: payload)
                    }
                case .confirmRevocation:
                    proxy.confirmRevocation(session: session, sequence: sequence, review: data, reply: completed)
                case .reviewAdoption:
                    proxy.reviewAdoption(session: session, sequence: sequence, request: data) { [weak self] echo, code, payload in
                        self?.receiveResponse(sequence: sequence, echo: echo, code: code, payload: payload)
                    }
                case .confirmAdoption:
                    proxy.confirmAdoption(session: session, sequence: sequence, review: data, reply: completed)
                }
                self.queue.asyncAfter(deadline: .now() + .seconds(15)) { [weak self] in
                    guard let self, self.pending?.sequence == sequence else { return }
                    self.finish(.expired)
                }
            } catch {
                self.notifications.async { reply(.failure(.invalidMessage)) }
            }
        }
    }

    private func receiveResponse(sequence: UInt64, echo: UInt64, code: UInt32, payload: Data?) {
        queue.async { [weak self] in
            guard let self, !self.gate.closed, let pending = self.pending else { return }
            let now = DispatchTime.now().uptimeNanoseconds
            let validCode: Bool
            switch pending.operation {
            case .preflight: validCode = (1...8).contains(code) && code != 6 && payload == nil
            case .consent: validCode = ExternalConsentStatus(code: code) != nil && payload == nil
            case .reviewRevocation: validCode = code == 39 && payload?.count == BootstrapWire.bytes
            case .confirmRevocation: validCode = ExternalRevocationOutcome(code: code) != nil && payload == nil
            case .reviewAdoption: validCode = code == 50 && payload?.count == BootstrapWire.bytes
            case .confirmAdoption: validCode = ExternalAdoptionOutcome(code: code) != nil && payload == nil
            }
            guard sequence == pending.sequence, echo == sequence, validCode,
                  self.connection.effectiveUserIdentifier == self.gate.account,
                  now >= pending.started, now - pending.started < PreflightGate.requestNanoseconds,
                  now >= self.gate.started, now - self.gate.started < BootstrapWire.lifetimeNanoseconds else {
                self.finish(.invalidMessage)
                return
            }
            self.pending = nil
            self.notifications.async { pending.reply(.success(BootstrapResponse(code: code, payload: payload))) }
        }
    }

    private func finish(_ failure: BootstrapFailure) {
        guard gate.close() else { return }
        session = nil
        let pending = pending
        self.pending = nil
        connection.invalidate()
        if let pending { notifications.async { pending.reply(.failure(failure)) } }
        emit(.closed(failure))
    }

    private func emit(_ value: BootstrapEvent) {
        // Consumer callbacks must not delay timeout or connection invalidation.
        notifications.async { [event] in event(value) }
    }
}
