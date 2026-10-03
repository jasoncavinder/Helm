import Foundation
import HelmExternalUpdateObservation

// QA-only host, never embedded in Helm. No database, target path, update request,
// process arguments or persisted authority is exchanged with the helper.
guard CommandLine.arguments.count == 1 else { exit(64) }
do {
    let client = try ExternalUpdaterBootstrapClient { event in
        switch event {
        case .ready:
            FileHandle.standardOutput.write(Data("{\"event\":\"authenticated_bootstrap_ready\"}\n".utf8))
            exit(0)
        case .closed(let failure):
            FileHandle.standardError.write(Data("Bootstrap rejected: \(failure.rawValue)\n".utf8))
            exit(1)
        }
    }
    client.begin()
    withExtendedLifetime(client) { RunLoop.main.run() }
} catch {
    FileHandle.standardError.write(Data("Bootstrap initialization rejected: \(error)\n".utf8))
    exit(1)
}
