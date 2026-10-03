import Foundation
import HelmExternalUpdateObservation

// VM-only QA host. Intent is sent only after authenticated readiness. No database,
// native trust assertion, saved permission or installer request is accepted.
let arguments = CommandLine.arguments
guard arguments.count == 1 || (arguments.count == 5 && ["--preflight", "--consent-status"].contains(arguments[1])) else { exit(64) }
let request = arguments.count == 5 ? ExternalPreflightRequest(
    targetPath: arguments[2], bundleIdentifier: arguments[3], installedBuild: arguments[4]
) : nil

final class Probe {
    var client: ExternalUpdaterBootstrapClient?

    func run() throws {
        client = try ExternalUpdaterBootstrapClient { [weak self] event in
            switch event {
            case .ready:
                FileHandle.standardOutput.write(Data("{\"event\":\"authenticated_bootstrap_ready\"}\n".utf8))
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
        client?.begin()
        withExtendedLifetime(self) { RunLoop.main.run() }
    }

    private func fail(_ failure: BootstrapFailure) {
        FileHandle.standardError.write(Data("Bootstrap rejected: \(failure.rawValue)\n".utf8))
        exit(1)
    }
}

let probe = Probe()
do { try probe.run() } catch {
    FileHandle.standardError.write(Data("Bootstrap initialization rejected: \(error)\n".utf8))
    exit(1)
}
