import Foundation
import XCTest
@testable import HelmExternalUpdateObservation

final class CaskArtifactPathsTests: XCTestCase {
    private let target = "/Applications/Example.app"

    private func json(_ value: Any) throws -> Data {
        try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys])
    }

    private func inspect(_ artifacts: [[String: Any]], target: String? = nil,
                         appdir: String = "/Applications") throws -> NativeCaskReceiptObserver.Artifacts {
        try NativeCaskReceiptObserver.inspect(receipt: json(["uninstall_artifacts": artifacts]),
            config: json(["default": ["appdir": appdir], "env": [:], "explicit": [:]]), target: target ?? self.target)
    }

    func testSuiteClaimsContainedAppWithoutMovedReference() throws {
        let result = try inspect([["suite": ["Archive/Tools"]]], target: "/Applications/Tools/Example.app")
        XCTAssertEqual(result.claims, ["/Applications/Tools"])
        XCTAssertNil(result.coverageGap)
        XCTAssertTrue(try inspect([["suite": ["Tool"]]], target: "/Applications/Tools/Example.app").claims.isEmpty)
    }

    func testSuiteRenamingAndSavedAppDirectory() throws {
        let data = try json(["uninstall_artifacts": [["suite": ["Archive/Old", ["target": "Tools"]]]]])
        let config = try json(["default": ["appdir": "/Wrong"], "env": ["appdir": "/AlsoWrong"],
                               "explicit": ["appdir": "/Applications"]])
        let result = try NativeCaskReceiptObserver.inspect(receipt: data, config: config, target: "/Applications/Tools/Example.app")
        XCTAssertEqual(result.claims, ["/Applications/Tools"])
        XCTAssertNil(result.coverageGap)
        XCTAssertEqual(try inspect([["suite": ["Old", ["target": "/Applications/Example.app"]]]]).claims, [target])
    }

    func testGenericArtifactRequiresExplicitAbsoluteTargetButNoAppDirectory() throws {
        for path in [target, target + "/Contents/component", "/Applications", "/"] {
            let data = try json(["uninstall_artifacts": [["artifact": ["payload", ["target": path]]]]])
            let result = try NativeCaskReceiptObserver.inspect(receipt: data, config: nil, target: target)
            XCTAssertEqual(result.claims, [path])
            XCTAssertNil(result.coverageGap)
        }
        let unrelated = try inspect([["artifact": ["payload", ["target": target + "-backup"]]]])
        XCTAssertTrue(unrelated.claims.isEmpty)
        XCTAssertNil(unrelated.coverageGap)
        for path in ["relative", ""] {
            XCTAssertEqual(try inspect([["artifact": ["payload", ["target": path]]]]).coverageGap, .uninspectedArtifacts)
        }
    }

    func testMovedAppAncestorClaimsAreAlsoConservativeDenials() throws {
        XCTAssertEqual(try inspect([["app": ["Outer.app"]]], target: "/Applications/Outer.app/Contents/Nested.app").claims,
                       ["/Applications/Outer.app"])
        XCTAssertEqual(try inspect([["artifact": ["payload", ["target": "/Applications/./Other/../Example.app"]]]]).claims, [target])
    }

    func testLiteralRemovalClaimsMatchSelfAncestorsAndDescendants() throws {
        for kind in ["uninstall", "zap"] {
            for directive in ["delete", "trash", "rmdir"] {
                for path in [target, target + "/Contents/data", "/Applications", "/"] {
                    let result = try inspect([[kind: [[directive: path]]]])
                    XCTAssertEqual(result.claims, [path], "\(kind) \(directive)")
                    XCTAssertNil(result.coverageGap)
                }
            }
        }
    }

    func testRemovalArraysRetainAllClaimsSortedAndDeduplicated() throws {
        let result = try inspect([["uninstall": [["delete": [target + "/Contents/data", target, target],
                                                  "trash": "/Elsewhere/Example.app", "rmdir": ["/Applications"]]]]])
        XCTAssertEqual(result.claims, ["/Applications", target, target + "/Contents/data"])
        XCTAssertNil(result.coverageGap)
    }

    func testCaseUnicodeAndComponentBoundariesApplyToEveryPathKind() throws {
        let kinds: [[String: Any]] = [["suite": ["eXAMPLE.APP"]],
            ["artifact": ["x", ["target": "/applications/eXAMPLE.APP"]]],
            ["zap": [["trash": "/applications/eXAMPLE.APP"]]]]
        for value in kinds {
            XCTAssertFalse(try inspect([value]).claims.isEmpty)
        }
        XCTAssertEqual(try inspect([["zap": [["trash": "/Applications/Caf\u{e9}.app"]]]],
                                  target: "/Applications/Cafe\u{301}.app").claims, ["/Applications/Caf\u{e9}.app"])
        XCTAssertTrue(try inspect([["zap": [["trash": [target + "-old", "/Application", "/Applications/Examples"]]]]]).claims.isEmpty)
    }

    func testUnresolvedRemovalSyntaxCannotEraseValidClaims() throws {
        for path in ["~/Applications/Example.app", "relative", "/Applications/*", "/Applications/Ex?mple.app",
                     "/Applications/[Ee]xample.app", "/Applications/{Example,Other}.app", "/Applications/\\*.app",
                     "/Applications/$APP", "/Applications/./Example.app", "/Applications/Other/../Example.app"] {
            let result = try inspect([["zap": [["trash": [path, target]]]]])
            XCTAssertEqual(result.claims, [target])
            XCTAssertEqual(result.coverageGap, .uninspectedArtifacts, path)
        }
    }

    func testUnknownSiblingDirectivesAndArtifactTypesRemainGaps() throws {
        for directive in ["script", "early_script", "pkgutil", "launchctl", "signal", "quit", "on_upgrade", "future"] {
            let result = try inspect([["uninstall": [["delete": target, directive: []]]]])
            XCTAssertEqual(result.claims, [target])
            XCTAssertEqual(result.coverageGap, .uninspectedArtifacts)
        }
        for kind in ["pkg", "installer", "binary", "preflight", "postflight_steps", "future"] {
            let result = try inspect([["suite": ["Example.app"]], [kind: ["anything"]]])
            XCTAssertEqual(result.claims, [target])
            XCTAssertEqual(result.coverageGap, .uninspectedArtifacts)
        }
    }

    func testMovedExpansionDependentPathsAndAppDirectoriesRemainGaps() throws {
        for kind in ["app", "suite", "artifact"] {
            for path in ["/Applications/*", "/Applications/$APP", "/Applications/#{app}", "/Applications/\\app"] {
                XCTAssertEqual(try inspect([[kind: ["source", ["target": path]]]]).coverageGap, .uninspectedArtifacts)
            }
            XCTAssertEqual(try inspect([[kind: ["*.app", ["target": target]]]]).coverageGap, .uninspectedArtifacts)
        }
        XCTAssertEqual(try inspect([["suite": ["Example.app"]]], appdir: "/Users/$USER/Applications").coverageGap,
                       .uninspectedArtifacts)
    }

    func testMalformedRecognizedDeclarationsFailInsteadOfReturningClearance() throws {
        for kind in ["suite", "artifact"] {
            for args: [Any] in [[], [1], [""], ["folder/"], [".."], ["app", ["target": 1]],
                                ["app", ["target": target, "unknown": true]], ["app", ["target": "~/App"]]] {
                XCTAssertThrowsError(try inspect([[kind: args]]), "\(kind) \(args)")
            }
        }
        XCTAssertThrowsError(try inspect([["artifact": ["payload"]]]))
        for value: Any in [[], [:], [["trash": 1]], [["trash": [target, 1]]], [["trash": NSNull()]],
                           [["trash": ""]], [["trash": "bad\npath"]], [["trash": target], ["delete": target]]] {
            XCTAssertThrowsError(try inspect([["zap": value]]))
        }
        XCTAssertEqual(try inspect([["zap": [["trash": []]]]]).coverageGap, .uninspectedArtifacts)
        XCTAssertEqual(try inspect([["uninstall": [[:]]]]).coverageGap, .uninspectedArtifacts)
    }

    func testRemovalAndAggregatePathBounds() throws {
        XCTAssertThrowsError(try inspect([["zap": [["trash": Array(repeating: target, count: 513)]]]]))
        XCTAssertThrowsError(try inspect([["zap": [["delete": String(repeating: "/", count: 4097)]]]]))
        let artifact: [String: Any] = ["uninstall": [["trash": Array(repeating: "/x", count: 512)]]]
        XCTAssertNil(try inspect(Array(repeating: artifact, count: 8)).coverageGap)
        XCTAssertThrowsError(try inspect(Array(repeating: artifact, count: 9))) {
            XCTAssertEqual($0 as? ObservationFailure, .limitExceeded)
        }
    }

    func testUnrelatedTokenContributesRemovalClaimAndRawSnapshotDrift() throws {
        let root = FileManager.default.homeDirectoryForCurrentUser.resolvingSymlinksInPath()
            .appendingPathComponent("helm-artifact-paths-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let metadata = root.appendingPathComponent("unrelated-token/.metadata", isDirectory: true)
        try FileManager.default.createDirectory(at: metadata, withIntermediateDirectories: true)
        let receipt = metadata.appendingPathComponent("INSTALL_RECEIPT.json", isDirectory: false)
        try json(["uninstall_artifacts": [["zap": [["trash": target]]]]]).write(to: receipt)
        let observer = NativeManagerObserver(caskrooms: [root])
        let app = URL(fileURLWithPath: target, isDirectory: true)
        let first = try observer.snapshot(target: app)
        let evidence = first.evidence(target: app, applicationRoots: [], hasStoreReceipt: false)
        XCTAssertEqual(evidence.disposition, .otherManager)
        XCTAssertEqual(evidence.homebrewReceiptClaims, [target])
        XCTAssertTrue(evidence.homebrewCoverageGaps.isEmpty)
        try json(["uninstall_artifacts": [["zap": [["trash": [target, "/unrelated/*"]]]]]]).write(to: receipt)
        let changed = try observer.snapshot(target: app)
        XCTAssertNotEqual(first, changed)
        let changedEvidence = changed.evidence(target: app, applicationRoots: [], hasStoreReceipt: false)
        XCTAssertEqual(changedEvidence.homebrewReceiptClaims, [target])
        XCTAssertEqual(changedEvidence.homebrewCoverageGaps.map(\.reason), [.uninspectedArtifacts])
    }

    func testEmptyMatchingClaimsStillCannotEstablishAuthority() throws {
        let result = try inspect([["suite": ["Other"]], ["artifact": ["x", ["target": "/Elsewhere"]]],
                                  ["zap": [["trash": "/Other/app"]]]])
        XCTAssertTrue(result.claims.isEmpty)
        XCTAssertNil(result.coverageGap)
        // The parser's narrowed gap is not a new complete-ownership disposition.
        let evidence = NativeManagerEvidence(exclusions: [], homebrewReferences: [], inspectedCaskEntries: 0)
        XCTAssertEqual(evidence.disposition, .unresolved)
    }
}
