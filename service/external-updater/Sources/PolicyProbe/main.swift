import Foundation
import HelmExternalUpdateObservation

// Development-only, no database or mutation. This cannot grant adoption/update
// consent, and a completed probe is not proof of operational helper readiness.
guard CommandLine.arguments.count == 2 else {
    FileHandle.standardError.write(Data("usage: helm-external-policy-probe /Applications/Example.app\n".utf8))
    exit(2)
}
do {
    let report = try NativeTargetObserver().observeForPolicy(path: CommandLine.arguments[1])
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
    FileHandle.standardOutput.write(try encoder.encode(report))
    FileHandle.standardOutput.write(Data("\n".utf8))
} catch {
    let code = (error as? ObservationFailure)?.rawValue ?? "observationFailed"
    FileHandle.standardError.write(Data("\(code)\n".utf8))
    exit(1)
}
