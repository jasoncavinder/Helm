import Darwin
import Foundation
import XCTest
import CExternalUpdatePolicy
@testable import HelmExternalUpdateObservation

final class CorePolicyTests: XCTestCase {
    func testABIV1LayoutMatchesRust() {
        XCTAssertEqual(MemoryLayout<HelmExternalNativeTarget>.size, 168)
        XCTAssertEqual(MemoryLayout<HelmExternalNativeTarget>.alignment, 8)
        XCTAssertEqual(MemoryLayout<HelmExternalNativeTarget>.offset(of: \.device), 24)
        XCTAssertEqual(MemoryLayout<HelmExternalNativeTarget>.offset(of: \.inode), 32)
        XCTAssertEqual(MemoryLayout<HelmExternalNativeTarget>.offset(of: \.ed25519_public_key), 104)
        XCTAssertEqual(MemoryLayout<HelmExternalNativeTarget>.offset(of: \.manager_exclusions), 144)
        XCTAssertEqual(MemoryLayout<HelmExternalNativeTarget>.offset(of: \.user_applications_root), 152)
    }
    private func evidence(path: String = "/Applications/Example.app", identifier: String = "org.example.App",
                          build: String = "100", team: String = "ABCDE12345", hash: [UInt8] = Array(repeating: 7, count: 20),
                          key: [UInt8] = Array(repeating: 8, count: 32), feed: String = "https://example.org/feed",
                          major: Int = 2, receipt: Bool = false, writable: Bool = false,
                          exclusions: [NativeManagerEvidence.Exclusion] = []) -> NativeTargetEvidence {
        NativeTargetEvidence(canonicalPath: path, device: 42, inode: 123, bundleIdentifier: identifier,
                             build: build, teamIdentifier: team, codeDirectoryHash: hash,
                             ed25519PublicKey: key, feedURL: feed, frameworkMajor: major,
                             hasStoreReceipt: receipt, writableByOthers: writable, inspectedEntries: 10,
                             managerEvidence: NativeManagerEvidence(exclusions: exclusions, homebrewReferences: [],
                                                                    inspectedCaskEntries: 2))
    }

    private func assess(_ evidence: NativeTargetEvidence) -> NativePolicyAssessment {
        NativePolicyAssessment.assess(evidence, userApplications: "/Users/agent/Applications")
    }

    func testRealRustBridgeNeverPromotesAnEmptyExclusionScan() throws {
        XCTAssertEqual(assess(evidence()), .unresolved)
        let report = NativePolicyReport(observation: evidence(), assessment: assess(evidence()))
        let json = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(report)) as? [String: Any])
        XCTAssertEqual(json["canUpdate"] as? Bool, false)
        XCTAssertEqual(json["assessment"] as? String, "unresolved")
        XCTAssertTrue(report.observation.requiresAuthorityResolution)
    }

    func testEveryNativeExclusionReachesRust() {
        for exclusion in [NativeManagerEvidence.Exclusion.appStoreReceipt, .homebrewCaskReference, .setappLocation, .setappBundleMarker,
                          .installerReceipt, .macportsLocation, .macportsRegistry] {
            XCTAssertEqual(assess(evidence(receipt: exclusion == .appStoreReceipt, exclusions: [exclusion])), .otherManager)
        }
        XCTAssertEqual(assess(evidence(receipt: true, exclusions: [.appStoreReceipt, .homebrewCaskReference, .setappLocation])), .otherManager)
        XCTAssertEqual(assess(evidence(receipt: true)), .invalidEvidence)
        XCTAssertEqual(assess(evidence(exclusions: [.appStoreReceipt])), .invalidEvidence)
    }

    func testRustRejectsUnsafeOrUnsupportedEvidenceAcrossTheABI() {
        for target in [evidence(writable: true), evidence(major: 1), evidence(hash: Array(repeating: 0, count: 20)),
                       evidence(key: [1]), evidence(feed: "http://example.org/feed"), evidence(team: "bad"),
                       evidence(identifier: "invalid"), evidence(build: "")] {
            XCTAssertEqual(assess(target), .unsupportedTarget)
        }
        XCTAssertEqual(assess(evidence(major: -1)), .invalidEvidence)
        XCTAssertEqual(assess(evidence(build: "100\0hidden")), .invalidEvidence)
        XCTAssertEqual(assess(evidence(build: String(repeating: "1", count: 129))), .invalidEvidence)
        XCTAssertEqual(assess(evidence(identifier: "com.jasoncavinder.Helm.QA")), .helmSelfUpdate)
    }

    func testCoreRootsCannotBeBroadenedByNativeFixtureRoots() {
        XCTAssertEqual(assess(evidence(path: "/Users/agent/Applications/Example.app")), .unresolved)
        XCTAssertEqual(assess(evidence(path: "/Users/other/Applications/Example.app")), .outsideRoots)
        XCTAssertEqual(assess(evidence(path: "/Applications/Host.app/Nested.app")), .outsideRoots)
        XCTAssertEqual(NativePolicyAssessment.assess(evidence(path: "/tmp/Example.app"), userApplications: "/tmp"), .outsideRoots)
        XCTAssertEqual(NativePolicyAssessment.assess(evidence(path: "/Users/agent/Applications/Example.app"), userApplications: nil), .outsideRoots)
    }

    func testFreshFilesystemObservationFlowsIntoRustAndUnreadableClaimsFailClosed() throws {
        let applications = try XCTUnwrap(NativeApplicationRoots.userApplications)
        let root = URL(fileURLWithPath: applications, isDirectory: true)
            .appendingPathComponent("HelmPolicy-\(UUID().uuidString)", isDirectory: true)
        let target = root.appendingPathComponent("Example.app", isDirectory: true)
        let resources = target.appendingPathComponent("Contents/Frameworks/Sparkle.framework/Resources")
        try FileManager.default.createDirectory(at: resources, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        XCTAssertEqual(chmod(root.path, 0o700), 0)
        try FileManager.default.createDirectory(at: target.appendingPathComponent("Contents/MacOS"), withIntermediateDirectories: true)
        try Data("fixture executable, never launched".utf8).write(to: target.appendingPathComponent("Contents/MacOS/Example"))
        let info: [String: Any] = ["CFBundleIdentifier": "org.example.App", "CFBundleVersion": "100", "CFBundleExecutable": "Example",
                                  "SUFeedURL": "https://example.org/feed",
                                  "SUPublicEDKey": Data(repeating: 8, count: 32).base64EncodedString()]
        try PropertyListSerialization.data(fromPropertyList: info, format: .xml, options: 0)
            .write(to: target.appendingPathComponent("Contents/Info.plist"))
        try PropertyListSerialization.data(fromPropertyList: ["CFBundleIdentifier": "org.sparkle-project.Sparkle",
                                                              "CFBundleShortVersionString": "2.9.5"], format: .xml, options: 0)
            .write(to: resources.appendingPathComponent("Info.plist"))
        let caskroom = root.appendingPathComponent("Caskroom", isDirectory: true)
        let observer = NativeTargetObserver(roots: [URL(fileURLWithPath: applications, isDirectory: true)],
                                            managers: NativeManagerObserver(caskrooms: [caskroom])) { _ in
            NativeSigningEvidence(identifier: "org.example.App", team: "ABCDE12345", hash: Data(repeating: 7, count: 20), info: info)
        }
        XCTAssertEqual(try observer.observeForPolicy(path: target.path).assessment, .unresolved)
        let version = caskroom.appendingPathComponent("example/100", isDirectory: true)
        try FileManager.default.createDirectory(at: version, withIntermediateDirectories: true)
        try FileManager.default.createSymbolicLink(at: version.appendingPathComponent("Example.app"), withDestinationURL: target)
        let excluded = try observer.observeForPolicy(path: target.path)
        XCTAssertEqual(excluded.assessment, .otherManager)
        XCTAssertFalse(excluded.canUpdate)
        // A scan error is not an empty scan or an unresolved-policy success.
        XCTAssertEqual(chmod(caskroom.path, 0), 0)
        defer { _ = chmod(caskroom.path, 0o700) }
        XCTAssertThrowsError(try observer.observeForPolicy(path: target.path))
    }
}
