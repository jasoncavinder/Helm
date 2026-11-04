import Foundation
import XCTest
@testable import HelmExternalUpdateObservation

final class CaskBinaryPathsTests: XCTestCase {
    private let target = "/Applications/Example.app"
    private var source: String { target + "/Contents/MacOS/tool" }

    private func json(_ value: Any) throws -> Data {
        try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys])
    }

    private func inspect(_ arguments: [Any], root: String? = nil, target: String? = nil,
                         siblings: [[String: Any]] = []) throws -> NativeCaskReceiptObserver.Artifacts {
        try NativeCaskReceiptObserver.inspect(
            receipt: json(["uninstall_artifacts": [["binary": arguments]] + siblings]),
            config: nil, target: target ?? self.target, caskroomPath: root)
    }

    func testAbsoluteSourceClaimsAppEvenWhenLinkIsElsewhere() throws {
        let result = try inspect([source, ["target": "/usr/local/bin/tool"]])
        XCTAssertEqual(result.claims, [source])
        XCTAssertNil(result.coverageGap)
    }

    func testBothEndpointsCanContributeAndAreDeduplicated() throws {
        let destination = target + "/Contents/Other"
        XCTAssertEqual(try inspect([source, ["target": destination]]).claims, [source, destination].sorted())
        XCTAssertEqual(try inspect([source, ["target": source]]).claims, [source])
    }

    func testLinkDestinationClaimsAppSelfAncestorAndDescendant() throws {
        for path in [target, target + "/Contents/tool", "/Applications", "/"] {
            let result = try inspect(["/Elsewhere/tool", ["target": path]])
            XCTAssertEqual(result.claims, [path])
            XCTAssertNil(result.coverageGap)
        }
    }

    func testDefaultAndEmptyTargetsUseEachNativeStandardPrefix() throws {
        for (root, prefix) in [("/opt/homebrew/Caskroom", "/opt/homebrew"), ("/usr/local/Caskroom", "/usr/local")] {
            for args: [Any] in [[source], [source, ["target": ""]], [source, ["target": "tool"]]] {
                let result = try inspect(args, root: root, target: prefix + "/bin/tool")
                XCTAssertEqual(result.claims, [prefix + "/bin/tool"])
                XCTAssertNil(result.coverageGap)
            }
            XCTAssertEqual(try inspect([source, ["target": "nested/renamed"]], root: root,
                                      target: prefix + "/bin/nested/renamed").claims, [prefix + "/bin/nested/renamed"])
        }
    }

    func testUnknownRootCannotGuessBinaryDirectoryButRetainsSourceClaim() throws {
        for root in [nil, "/custom/Caskroom", "/opt/homebrew/Caskroom-other", "/opt/homebrew/Caskroom/../Caskroom"] {
            let result = try inspect([source], root: root)
            XCTAssertEqual(result.claims, [source])
            XCTAssertEqual(result.coverageGap, .uninspectedArtifacts)
            XCTAssertNil(try inspect([source, ["target": "/elsewhere/tool"]], root: root).coverageGap)
        }
    }

    func testSavedConfigAndReceiptArchitectureCannotOverrideNativePrefix() throws {
        let receipt = try json(["arch": "x86_64", "homebrew_prefix": "/Applications",
                                "uninstall_artifacts": [["binary": [source]]]])
        let config = try json(["default": ["binarydir": "/Applications"], "env": [:], "explicit": [:]])
        let result = try NativeCaskReceiptObserver.inspect(receipt: receipt, config: config,
            target: "/opt/homebrew/bin/tool", caskroomPath: "/opt/homebrew/Caskroom")
        XCTAssertEqual(result.claims, ["/opt/homebrew/bin/tool"])
        XCTAssertNil(result.coverageGap)
    }

    func testRelativeSourceRemainsGapWithoutHidingExplicitTargetClaim() throws {
        for source in ["tool", "Example.app/Contents/MacOS/tool", "../Other/tool", "~/Applications/Example.app/tool"] {
            let result = try inspect([source, ["target": target]])
            XCTAssertEqual(result.claims, [target])
            XCTAssertEqual(result.coverageGap, .uninspectedArtifacts)
        }
    }

    func testRelativeSourceCanRetainDerivedTargetClaimButNeverClearGap() throws {
        let result = try inspect(["archive/tool"], root: "/usr/local/Caskroom", target: "/usr/local/bin/tool")
        XCTAssertEqual(result.claims, ["/usr/local/bin/tool"])
        XCTAssertEqual(result.coverageGap, .uninspectedArtifacts)
    }

    func testUnresolvedEndpointNeverHidesOtherLiteralEndpoint() throws {
        for path in ["/Applications/*/tool", "/Applications/Ex?mple.app/tool", "/Applications/[Ee]xample.app/tool",
                     "/Applications/{Example,Other}.app/tool", "/Applications/$APP/tool", "/Applications/\\app/tool",
                     "~/Applications/Example.app/tool", "/Applications/./Example.app/tool", "/Applications/Other/../Example.app/tool"] {
            let badSource = try inspect([path, ["target": target]])
            XCTAssertEqual(badSource.claims, [target])
            XCTAssertEqual(badSource.coverageGap, .uninspectedArtifacts)
            let badTarget = try inspect([source, ["target": path]], root: "/opt/homebrew/Caskroom")
            XCTAssertEqual(badTarget.claims, [source])
            XCTAssertEqual(badTarget.coverageGap, .uninspectedArtifacts)
        }
    }

    func testCaseUnicodeAndComponentBoundariesRemainDenialOnly() throws {
        XCTAssertEqual(try inspect(["/applications/eXAMPLE.APP/tool", ["target": "/other/tool"]]).claims,
                       ["/applications/eXAMPLE.APP/tool"])
        XCTAssertEqual(try inspect(["/Applications/Caf\u{e9}.app/tool", ["target": "/other/tool"]],
                                  target: "/Applications/Cafe\u{301}.app").claims, ["/Applications/Caf\u{e9}.app/tool"])
        let unrelated = try inspect([target + "-old/tool", ["target": "/Applications/Examples"]])
        XCTAssertTrue(unrelated.claims.isEmpty)
        XCTAssertNil(unrelated.coverageGap)
    }

    func testUnsupportedSiblingsKeepTheirGapAndBinaryClaim() throws {
        for kind in ["pkg", "installer", "postflight_steps", "bash_completion", "future"] {
            let result = try inspect([source, ["target": "/other/tool"]], siblings: [[kind: ["anything"]]])
            XCTAssertEqual(result.claims, [source])
            XCTAssertEqual(result.coverageGap, .uninspectedArtifacts)
        }
    }

    func testMalformedBinaryDoesNotBecomeEmptySuccessfulScan() throws {
        for args: [Any] in [[], [1], [""], ["/"], ["folder/"], [".."], ["/bad\npath"],
                            [source, "other"], [source, [:]], [source, ["target": 1]],
                            [source, ["target": NSNull()]], [source, ["target": target, "extra": true]],
                            [source, ["target": target], "extra"], [source, ["target": "bad\u{0}path"]]] {
            XCTAssertThrowsError(try inspect(args), "\(args)")
        }
    }

    func testPathAndReceiptBoundsStillApply() throws {
        XCTAssertThrowsError(try inspect(["/" + String(repeating: "x", count: 4096)]))
        XCTAssertThrowsError(try inspect([source, ["target": String(repeating: "x", count: 4097)]]))
        // A bounded input can still exceed the bound after adding the native prefix.
        XCTAssertThrowsError(try inspect([source, ["target": String(repeating: "x", count: 4096)]],
                                        root: "/opt/homebrew/Caskroom"))
        let artifact: [String: Any] = ["binary": [source, ["target": "/other/tool"]]]
        XCTAssertThrowsError(try inspect([source], siblings: Array(repeating: artifact, count: 512)))
    }

    func testBinaryParserDoesNotAskForAppDirectoryOrProbeEndpoints() throws {
        let result = try NativeCaskArtifactPaths.inspect(kind: "binary",
            value: ["/no-such-volume/Example.app/tool", ["target": "/no-such-volume/bin/tool"]]) {
            XCTFail("Binary paths do not depend on appdir")
            throw ObservationFailure.unreadableManagerEvidence
        }
        XCTAssertEqual(result.paths, ["/no-such-volume/Example.app/tool", "/no-such-volume/bin/tool"])
        XCTAssertFalse(result.incomplete)
    }

    func testNativeSnapshotFindsBinaryOnlyClaimInUnrelatedTokenAndBindsRawDrift() throws {
        let root = FileManager.default.homeDirectoryForCurrentUser.resolvingSymlinksInPath()
            .appendingPathComponent("helm-binary-receipts-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let metadata = root.appendingPathComponent("unrelated/.metadata", isDirectory: true)
        try FileManager.default.createDirectory(at: metadata, withIntermediateDirectories: true)
        let receipt = metadata.appendingPathComponent("INSTALL_RECEIPT.json", isDirectory: false)
        try json(["uninstall_artifacts": [["binary": [source, ["target": "/other/tool"]]]]]).write(to: receipt)
        let observer = NativeManagerObserver(caskrooms: [root])
        let app = URL(fileURLWithPath: target, isDirectory: true)
        let first = try observer.snapshot(target: app)
        let evidence = first.evidence(target: app, applicationRoots: [], hasStoreReceipt: false)
        XCTAssertEqual(evidence.disposition, .otherManager)
        XCTAssertEqual(evidence.exclusions, [.homebrewCaskReceipt])
        XCTAssertEqual(evidence.homebrewReceiptClaims, [source])
        XCTAssertTrue(evidence.homebrewCoverageGaps.isEmpty)
        try json(["uninstall_artifacts": [["binary": [source, ["target": "/other/renamed"]]]]]).write(to: receipt)
        let changed = try observer.snapshot(target: app)
        XCTAssertNotEqual(first, changed)
        XCTAssertEqual(first.receipts.values.first?.claims, changed.receipts.values.first?.claims)
    }

    func testNoMatchingClaimsIsNotOwnershipClearance() throws {
        let result = try inspect(["/Other/tool", ["target": "/other/link"]])
        XCTAssertTrue(result.claims.isEmpty)
        XCTAssertNil(result.coverageGap)
        XCTAssertEqual(NativeManagerEvidence(exclusions: [], homebrewReferences: [], inspectedCaskEntries: 0).disposition, .unresolved)
    }
}
