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
    private var pending: (sequence: UInt64, started: UInt64, consent: Bool, reply: (Result<UInt32, BootstrapFailure>) -> Void)?

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
        send(request, consent: false) { reply($0.map { NativePolicyAssessment(code: $0) }) }
    }

    public func consentStatus(_ request: ExternalPreflightRequest,
                              reply: @escaping (Result<ExternalConsentStatus, BootstrapFailure>) -> Void) {
        send(request, consent: true) { result in
            reply(result.flatMap { code in
                ExternalConsentStatus(code: code).map { .success($0) } ?? .failure(.invalidMessage)
            })
        }
    }

    private func send(_ request: ExternalPreflightRequest, consent: Bool,
                      reply: @escaping (Result<UInt32, BootstrapFailure>) -> Void) {
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
                let data = try JSONEncoder().encode(request)
                _ = try ExternalPreflightRequest.decode(data, userApplications: NativeApplicationRoots.userApplications)
                let sequence = self.nextSequence
                self.nextSequence += 1
                self.pending = (sequence, DispatchTime.now().uptimeNanoseconds, consent, reply)
                guard let proxy = self.connection.remoteObjectProxyWithErrorHandler({ [weak self] _ in
                    self?.close(.transport)
                }) as? ExternalUpdaterBootstrapProtocol else {
                    self.finish(.transport)
                    return
                }
                let completed: (UInt64, UInt32) -> Void = { [weak self] echo, code in
                    self?.receivePreflight(sequence: sequence, echo: echo, code: code)
                }
                if consent {
                    proxy.consentStatus(session: session, sequence: sequence, request: data, reply: completed)
                } else {
                    proxy.preflight(session: session, sequence: sequence, request: data, reply: completed)
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

    private func receivePreflight(sequence: UInt64, echo: UInt64, code: UInt32) {
        queue.async { [weak self] in
            guard let self, !self.gate.closed, let pending = self.pending else { return }
            let now = DispatchTime.now().uptimeNanoseconds
            let validCode = pending.consent ? ExternalConsentStatus(code: code) != nil : (1...8).contains(code) && code != 6
            guard sequence == pending.sequence, echo == sequence, validCode,
                  self.connection.effectiveUserIdentifier == self.gate.account,
                  now >= pending.started, now - pending.started < PreflightGate.requestNanoseconds,
                  now >= self.gate.started, now - self.gate.started < BootstrapWire.lifetimeNanoseconds else {
                self.finish(.invalidMessage)
                return
            }
            self.pending = nil
            self.notifications.async { pending.reply(.success(code)) }
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
