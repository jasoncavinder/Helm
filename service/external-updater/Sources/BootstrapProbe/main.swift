import Foundation
import HelmExternalUpdateObservation

// VM-only QA host. Intent is sent only after authenticated readiness. No database,
// native trust assertion, permission grant or installer request is accepted.
let bundledService = CommandLine.arguments.dropFirst().first == "--bundled-service"
let arguments = bundledService
    ? [CommandLine.arguments[0]] + Array(CommandLine.arguments.dropFirst(2)) : CommandLine.arguments
guard arguments.count == 1 || (arguments.count == 5 && ["--preflight", "--consent-status"].contains(arguments[1]))
    || (arguments.count == 3 && ["--review-revocation", "--revoke-consent"].contains(arguments[1])) else { exit(64) }
let request = arguments.count == 5 ? ExternalPreflightRequest(
    targetPath: arguments[2], bundleIdentifier: arguments[3], installedBuild: arguments[4]
) : nil

final class Probe {
    var client: ExternalUpdaterBootstrapClient?

    func run() throws {
        let handler: (BootstrapEvent) -> Void = { [weak self] event in
            switch event {
            case .ready:
                FileHandle.standardOutput.write(Data("{\"event\":\"authenticated_bootstrap_ready\"}\n".utf8))
                if arguments.count == 3 {
                    self?.client?.reviewRevocation(ExternalRevocationRequest(targetPath: arguments[2])) { result in
                        switch result {
                        case .success(let review):
                            if arguments[1] == "--review-revocation" {
                                self?.report("revocation_review_completed", status: "reviewed")
                                return
                            }
                            self?.client?.confirmRevocation(review) { result in
                                switch result {
                                case .success(let outcome): self?.report("revocation_completed", status: outcome.rawValue)
                                case .failure(let failure):
                                    FileHandle.standardError.write(Data("Revocation outcome unknown; do not retry automatically.\n".utf8))
                                    self?.fail(failure)
                                }
                            }
                        case .failure(let failure): self?.fail(failure)
                        }
                    }
                    return
                }
                guard let request else { exit(0) }
                if arguments[1] == "--consent-status" {
                    self?.client?.consentStatus(request) { result in
                        switch result {
                        case .success(let status):
                            let report: [String: Any] = ["event": "consent_status_completed", "status": status.rawValue, "canUpdate": false]
                            if let data = try? JSONSerialization.data(withJSONObject: report, options: [.sortedKeys]) {
                                FileHandle.standardOutput.write(data)
                                FileHandle.standardOutput.write(Data("\n".utf8))
                                exit(0)
                            }
                            exit(1)
                        case .failure(let failure): self?.fail(failure)
                        }
                    }
                    return
                }
                self?.client?.preflight(request) { result in
                    switch result {
                    case .success(let assessment):
                        let report: [String: Any] = ["event": "preflight_completed", "assessment": assessment.rawValue, "canUpdate": false]
                        if let data = try? JSONSerialization.data(withJSONObject: report, options: [.sortedKeys]) {
                            FileHandle.standardOutput.write(data)
                            FileHandle.standardOutput.write(Data("\n".utf8))
                            exit(0)
                        }
                        exit(1)
                    case .failure(let failure): self?.fail(failure)
                    }
                }
            case .closed(let failure): self?.fail(failure)
            }
        }
        client = try bundledService ? ExternalUpdaterBootstrapClient.bundledService(event: handler)
            : ExternalUpdaterBootstrapClient(event: handler)
        client?.begin()
        withExtendedLifetime(self) { RunLoop.main.run() }
    }

    private func fail(_ failure: BootstrapFailure) {
        FileHandle.standardError.write(Data("Bootstrap rejected: \(failure.rawValue)\n".utf8))
        exit(1)
    }

    private func report(_ event: String, status: String) {
        let report: [String: Any] = ["event": event, "status": status, "canUpdate": false]
        guard let data = try? JSONSerialization.data(withJSONObject: report, options: [.sortedKeys]) else { exit(1) }
        FileHandle.standardOutput.write(data)
        FileHandle.standardOutput.write(Data("\n".utf8))
        exit(0)
    }
}

let probe = Probe()
do { try probe.run() } catch {
    FileHandle.standardError.write(Data("Bootstrap initialization rejected: \(error)\n".utf8))
    exit(1)
}
