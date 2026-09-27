import Foundation

extension HelmCore {
    func prepareRuntime(service: HelmServiceProtocol, generation: UInt64) {
        withTimeout(
            5,
            source: "core.xpc",
            action: "connectionHandshake",
            taskType: "connection",
            operation: { completion in
                // Keep legacy entry until the real first-run value flow is ready.
                // Research previews must not persist product acknowledgment.
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
