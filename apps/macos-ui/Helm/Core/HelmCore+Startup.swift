import Foundation

extension HelmCore {
    func prepareRuntime(service: HelmServiceProtocol, generation: UInt64) {
        if ProductionFirstRunGate.isEnabled {
            prepareProductionFirstRun(service: service, generation: generation)
            return
        }
        withTimeout(
            5,
            source: "core.xpc",
            action: "connectionHandshake",
            taskType: "connection",
            operation: { completion in
                // Debug research/preview compatibility only. Normal builds use
                // versioned entry; previews must not persist its acknowledgment.
                service.prepareStartup(requireFirstRunAcknowledgment: false, withReply: completion)
            }
        ) { [weak self] json in
            DispatchQueue.main.async {
                guard let self, generation == self.connectionGeneration, self.connection != nil else { return }
                guard let snapshot = ServiceStartupSnapshot.decode(json, requiringAcknowledgment: false),
                      snapshot.permitsRuntimeActivation else {
                    self.handleConnectionFailure(generation: generation)
                    return
                }
                self.activatePreparedRuntime(service: service, generation: generation, snapshot: snapshot)
            }
        }
    }

    func retryProductionFirstRun() {
        guard ProductionFirstRunGate.isEnabled else { return }
        guard let service = connection?.remoteObjectProxy as? HelmServiceProtocol else {
            setupConnection()
            return
        }
        prepareProductionFirstRun(service: service, generation: connectionGeneration)
    }

    private func prepareProductionFirstRun(service: HelmServiceProtocol, generation: UInt64) {
        func jsonRequest(_ action: String, timeout: TimeInterval = 10, operation: @escaping (@escaping (String?) -> Void) -> Void,
                         reply: @escaping (String?) -> Void) {
            withTimeout(timeout, source: "core.firstRun", action: action, operation: operation) { [weak self] value in
                FirstRunReplyDelivery.deliver(value, isCurrent: { [weak self] in
                    guard let self else { return false }
                    return generation == self.connectionGeneration && self.connection != nil
                }, reply: reply)
            }
        }
        func boolRequest(_ action: String, operation: @escaping (@escaping (Bool) -> Void) -> Void,
                         reply: @escaping (Bool) -> Void) {
            withTimeout(30, source: "core.firstRun", action: action,
                        operation: { completion in operation { completion($0) } }) { [weak self] value in
                FirstRunReplyDelivery.deliver(value == true, isCurrent: { [weak self] in
                    guard let self else { return false }
                    return generation == self.connectionGeneration && self.connection != nil
                }, reply: reply)
            }
        }
        productionFirstRun.begin(
            client: .init(
                prepare: { reply in
                    jsonRequest("prepare", operation: {
                        service.prepareStartup(requireFirstRunAcknowledgment: true, withReply: $0)
                    }, reply: reply)
                },
                acceptTerms: { version, reply in
                    boolRequest("acceptTerms", operation: {
                        service.acceptFirstRunLicenseTerms(version: version, withReply: $0)
                    }, reply: reply)
                },
                observe: { reply in
                    jsonRequest("observe", operation: service.observeFirstRunEnvironment, reply: reply)
                },
                acknowledge: { experience, reply in
                    boolRequest("acknowledge", operation: {
                        service.acknowledgeFirstRunExperience(experienceId: experience, withReply: $0)
                    }, reply: reply)
                },
                activate: { reply in
                    boolRequest("activate", operation: { [weak self] completion in
                        guard let self, generation == self.connectionGeneration, self.connection != nil else {
                            completion(false)
                            return
                        }
                        service.startRuntimeWithDiscovery(
                            networkAvailable: self.networkAvailability == .available,
                            withReply: completion
                        )
                    }, reply: reply)
                },
                reviewRepair: { reply in
                    jsonRequest("reviewRepair", timeout: 30, operation: service.reviewFirstRunRepair, reply: reply)
                },
                applyRepair: { token, reply in
                    jsonRequest("applyRepair", timeout: 30, operation: {
                        service.applyFirstRunRepair(reviewToken: token, withReply: $0)
                    }, reply: reply)
                }
            ),
            requiresLegalAcceptance: Self.requiresLicenseTermsAcceptance(
                channel: HelmDistributionChannel.from(), acceptedVersion: nil
            ),
            localAcceptedTerms: acceptedLicenseTermsVersion
        ) { [weak self] in
            guard let self, let snapshot = self.productionFirstRun.snapshot else { return }
            self.applyPreparedLicenseTerms(snapshot.acceptedLicenseTermsVersion)
            self.completeConnectionHandshake(generation: generation, safeModeEnabled: snapshot.safeMode)
        }
    }

    private func activatePreparedRuntime(
        service: HelmServiceProtocol,
        generation: UInt64,
        snapshot: ServiceStartupSnapshot
    ) {
        withTimeout(
            30,
            source: "core.xpc",
            action: "startRuntime",
            taskType: "connection",
            operation: { completion in service.startRuntime(withReply: completion) }
        ) { [weak self] started in
            DispatchQueue.main.async {
                guard let self, generation == self.connectionGeneration, self.connection != nil else { return }
                guard started == true else {
                    self.handleConnectionFailure(generation: generation)
                    return
                }
                self.completeConnectionHandshake(generation: generation, safeModeEnabled: snapshot.safeMode)
            }
        }
    }
}
