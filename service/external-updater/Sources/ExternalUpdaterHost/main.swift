import Foundation
import HelmExternalUpdateObservation
import Sparkle

// Inspection, read-only app preflight and private storage preparation. Never initialize
// SPUUpdater or accept a feed, install command or authorization grant.
let serviceLaunch = CommandLine.arguments.count == 1
let mode = CommandLine.arguments.count == 2 ? CommandLine.arguments[1] : nil
guard serviceLaunch || mode.map({ ["--preflight", "--serve-bootstrap", "--prepare-ledger"].contains($0) }) == true else {
    FileHandle.standardError.write(Data("Use --preflight, --serve-bootstrap or --prepare-ledger; direct updates are disabled.\n".utf8))
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
    let bundledService = URL(fileURLWithPath: helper.canonicalPath).pathExtension == "xpc"
    guard !serviceLaunch || bundledService,
          mode != "--serve-bootstrap" || !bundledService else {
        throw HelperObservationFailure.invalidMetadata
    }
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
    if mode == "--prepare-ledger" {
        try NativeHelperLedger(identity: helper).prepare()
        FileHandle.standardOutput.write(Data("{\"ledgerReady\":true,\"directUpdatesEnabled\":false}\n".utf8))
    } else if mode == "--preflight" {
        let report = PreflightReport(helper: helper, frameworkPath: frameworkPath, frameworkVersion: "2.9.5")
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        FileHandle.standardOutput.write(try encoder.encode(report))
        FileHandle.standardOutput.write(Data("\n".utf8))
    } else if bundledService {
        try ExternalUpdaterPrivateServiceHost.run(identity: helper) { event in
            if case .ready = event {
                FileHandle.standardOutput.write(Data("{\"event\":\"authenticated_caller_ready\"}\n".utf8))
            }
        }
    } else {
        let host = try BootstrapHost(identity: helper)
        host.run()
    }
} catch {
    FileHandle.standardError.write(Data("External updater preflight rejected: \(error)\n".utf8))
    exit(1)
}
