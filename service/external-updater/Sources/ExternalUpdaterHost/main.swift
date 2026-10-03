import Foundation
import HelmExternalUpdateObservation
import Sparkle

// Inspection and authenticated read-only app preflight only. Never initialize
// SPUUpdater or accept a feed, install command or authorization grant.
guard CommandLine.arguments.count == 2,
      ["--preflight", "--serve-bootstrap"].contains(CommandLine.arguments[1]) else {
    FileHandle.standardError.write(Data("Use --preflight or --serve-bootstrap; direct updates are disabled.\n".utf8))
    exit(64)
}

struct PreflightReport: Encodable {
    let helper: NativeHelperEvidence
    let frameworkPath: String
    let frameworkVersion: String
    let directUpdatesEnabled = false
}

do {
    let helper = try NativeHelperObserver().observeSelf()
    let framework = Bundle(for: SPUUpdater.self)
    let frameworkPath = framework.bundleURL.resolvingSymlinksInPath().path
    let expected = URL(fileURLWithPath: helper.canonicalPath)
        .appendingPathComponent("Contents/Frameworks/Sparkle.framework")
        .resolvingSymlinksInPath().path
    guard expected.hasPrefix(helper.canonicalPath + "/Contents/Frameworks/"), frameworkPath == expected,
          framework.bundleIdentifier == "org.sparkle-project.Sparkle",
          framework.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String == "2.9.5" else {
        throw HelperObservationFailure.invalidMetadata
    }
    if CommandLine.arguments[1] == "--preflight" {
        let report = PreflightReport(helper: helper, frameworkPath: frameworkPath, frameworkVersion: "2.9.5")
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        FileHandle.standardOutput.write(try encoder.encode(report))
        FileHandle.standardOutput.write(Data("\n".utf8))
    } else {
        let host = try BootstrapHost(identity: helper)
        host.run()
    }
} catch {
    FileHandle.standardError.write(Data("External updater preflight rejected: \(error)\n".utf8))
    exit(1)
}
