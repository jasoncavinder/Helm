import Foundation
import HelmExternalUpdateObservation

// Development-only, read-only VM probe. It is not embedded in Helm or an updater.
guard CommandLine.arguments.count == 2 else {
    FileHandle.standardError.write(Data("usage: helm-external-observe /Applications/Example.app\n".utf8))
    exit(2)
}
do {
    let evidence = try NativeTargetObserver().observe(path: CommandLine.arguments[1])
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
    FileHandle.standardOutput.write(try encoder.encode(evidence))
    FileHandle.standardOutput.write(Data("\n".utf8))
} catch {
    let code = (error as? ObservationFailure)?.rawValue ?? "observationFailed"
    FileHandle.standardError.write(Data("\(code)\n".utf8))
    exit(1)
}
