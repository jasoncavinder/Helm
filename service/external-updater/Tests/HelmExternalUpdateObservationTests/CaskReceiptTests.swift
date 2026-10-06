import Darwin
import Foundation
import XCTest
@testable import HelmExternalUpdateObservation

final class CaskReceiptTests: XCTestCase {
    private var root: URL!
    private var token: URL!
    private var metadata: URL!
    private let target = "/Applications/Example.app"

    override func setUpWithError() throws {
        root = FileManager.default.homeDirectoryForCurrentUser.resolvingSymlinksInPath()
            .appendingPathComponent("helm-cask-receipts-\(UUID().uuidString)", isDirectory: true)
        token = root.appendingPathComponent("Caskroom/example", isDirectory: true)
        metadata = token.appendingPathComponent(".metadata", isDirectory: true)
        try FileManager.default.createDirectory(at: metadata, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        if let root { try FileManager.default.removeItem(at: root) }
    }

    private func json(_ value: Any) throws -> Data {
        try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys])
    }

    private func config(_ base: String = "/Applications", env: [String: Any] = [:],
                        explicit: [String: Any] = [:]) throws -> Data {
        try json(["default": ["appdir": base], "env": env, "explicit": explicit])
    }

    private func receipt(_ arguments: [Any] = ["Example.app"]) throws -> Data {
        try json(["uninstall_artifacts": [["app": arguments]]])
    }

    private func claims(_ arguments: [Any] = ["Example.app"], configuration: Data? = nil,
                        target: String? = nil) throws -> [String] {
        try NativeCaskReceiptObserver.claims(receipt: receipt(arguments), config: configuration ?? config(), target: target ?? self.target)
    }

    private func install(_ receipt: Data? = nil, configuration: Data? = nil) throws {
        try (receipt ?? self.receipt()).write(to: metadata.appendingPathComponent("INSTALL_RECEIPT.json", isDirectory: false))
        try (configuration ?? config()).write(to: metadata.appendingPathComponent("config.json", isDirectory: false))
    }

    private func snapshot(budget: Int = 4 * 1024 * 1024) throws -> NativeCaskReceiptObserver.Snapshot {
        try NativeCaskReceiptObserver.snapshot(token: token, target: URL(fileURLWithPath: target, isDirectory: false), remainingBytes: budget)
    }

    func testReceiptClaimWithoutVersionDirectoryOrReference() throws {
        try install()
        let scanner = NativeManagerObserver(caskrooms: [token.deletingLastPathComponent()])
        let target = URL(fileURLWithPath: target, isDirectory: false)
        let snapshot = try scanner.snapshot(target: target)
        let evidence = snapshot.evidence(target: target, applicationRoots: [], hasStoreReceipt: false)
        XCTAssertEqual(evidence.exclusions, [.homebrewCaskReceipt])
        XCTAssertEqual(evidence.disposition, .otherManager)
        XCTAssertEqual(evidence.homebrewReceiptClaims, [self.target])
        XCTAssertTrue(evidence.homebrewReferences.isEmpty)
        let encoded = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(evidence)) as? [String: Any])
        XCTAssertEqual(encoded["homebrewReceiptClaims"] as? [String], [self.target])
    }

    func testSourceBasenameAndRenamedAbsoluteOrRelativeDestinations() throws {
        XCTAssertEqual(try claims(["Archive/Nested/Example.app"]), [target])
        XCTAssertEqual(try claims(["Old.app", ["target": "Example.app"]]), [target])
        XCTAssertEqual(try claims(["Old.app", ["target": target]]), [target])
        XCTAssertEqual(try claims(["Example.app", ["target": ""]]), [target])
        XCTAssertEqual(try claims(["Old.app", ["target": "/Applications/./Other/../Example.app"]]), [target])
        XCTAssertEqual(try claims(["Old.app", ["target": "Example.app/Contents/Nested.app"]]), [target + "/Contents/Nested.app"])
    }

    func testSavedAppDirectoryPrecedenceAndOtherDirectories() throws {
        XCTAssertTrue(try claims(configuration: config("/Other")).isEmpty)
        XCTAssertEqual(try claims(configuration: config("/Other", env: ["appdir": "/Applications"])), [target])
        XCTAssertEqual(try claims(configuration: config("/Other", env: ["appdir": "/Elsewhere"], explicit: ["appdir": "/Applications"])), [target])
        XCTAssertTrue(try claims(configuration: config("/Applications", explicit: ["appdir": "/Elsewhere"])).isEmpty)
        let defaults = try json(["default": [:], "env": [:], "explicit": [:]])
        XCTAssertEqual(try claims(configuration: defaults), [target])
        XCTAssertEqual(try claims(configuration: config("/Users/another/Applications"),
                                  target: "/Users/another/Applications/Example.app"), ["/Users/another/Applications/Example.app"])
    }

    func testCaseAndCanonicalEquivalenceAreConservativeDenials() throws {
        XCTAssertEqual(try claims(["eXAMPLE.APP"], configuration: config("/applications")), ["/applications/eXAMPLE.APP"])
        XCTAssertEqual(try claims(["Caf\u{e9}.app"], target: "/Applications/Cafe\u{301}.app"), ["/Applications/Caf\u{e9}.app"])
        XCTAssertTrue(try claims(["Example.app-backup"]).isEmpty)
        XCTAssertTrue(try claims(["Example Other.app"]).isEmpty)
    }

    func testRetainsEveryMatchingAppAndIgnoresNonAppArtifacts() throws {
        let data = try json(["uninstall_artifacts": [
            ["app": ["Example.app"]], ["app": ["Old.app", ["target": target]]],
            ["app": ["Other.app"]], ["uninstall": [["script": "/not/executed"]]], ["pkg": ["Example.pkg"]]
        ]])
        XCTAssertEqual(try NativeCaskReceiptObserver.claims(receipt: data, config: config(), target: target), [target])
    }

    func testMissingAndLegacyReceiptsRemainUnresolved() throws {
        XCTAssertTrue(try snapshot().claims.isEmpty)
        for data in [try json([:]), try json(["uninstall_artifacts": NSNull()]), try json(["uninstall_artifacts": []])] {
            XCTAssertTrue(try NativeCaskReceiptObserver.claims(receipt: data, config: nil, target: target).isEmpty)
        }
        let empty = try NativeManagerObserver(caskrooms: [token.deletingLastPathComponent()]).snapshot(
            target: URL(fileURLWithPath: target, isDirectory: false))
        XCTAssertEqual(empty.evidence(target: URL(fileURLWithPath: target, isDirectory: false),
                                      applicationRoots: [], hasStoreReceipt: false).disposition, .unresolved)
    }

    func testMissingMetadataAndReceiptHaveDistinctCoverageGaps() throws {
        XCTAssertEqual(try snapshot().coverageGap, .missingReceipt)
        try FileManager.default.removeItem(at: metadata)
        XCTAssertEqual(try snapshot().coverageGap, .missingMetadata)
    }

    func testLegacyEmptyAndUninspectedArtifactsRetainCoverageGaps() throws {
        for (value, gap): (Any, NativeCaskCoverageGap.Reason) in [
            ([:], .missingArtifactDeclarations), (["uninstall_artifacts": NSNull()], .missingArtifactDeclarations),
            (["uninstall_artifacts": []], .emptyArtifactDeclarations),
            (["uninstall_artifacts": [["pkg": ["Example.pkg"]]]], .uninspectedArtifacts),
            (["uninstall_artifacts": [["future-artifact": ["Example.app"]]]], .uninspectedArtifacts)
        ] {
            try install(json(value))
            let observed = try snapshot()
            XCTAssertEqual(observed.coverageGap, gap)
            XCTAssertTrue(observed.claims.isEmpty)
        }
    }

    func testInspectedAppDestinationsAreNotCompleteOwnershipProof() throws {
        try install(receipt(["Other.app"]))
        let target = URL(fileURLWithPath: target, isDirectory: false)
        let observed = try NativeManagerObserver(caskrooms: [token.deletingLastPathComponent()]).snapshot(target: target)
        let evidence = observed.evidence(target: target, applicationRoots: [], hasStoreReceipt: false)
        XCTAssertTrue(evidence.homebrewCoverageGaps.isEmpty)
        XCTAssertTrue(evidence.homebrewReceiptClaims.isEmpty)
        XCTAssertEqual(evidence.disposition, .unresolved)
    }

    func testMatchingAppClaimSurvivesUninspectedArtifactsAndGapIsEncoded() throws {
        try install(json(["uninstall_artifacts": [["app": ["Example.app"]], ["pkg": ["Other.pkg"]]]]))
        let target = URL(fileURLWithPath: target, isDirectory: false)
        let observed = try NativeManagerObserver(caskrooms: [token.deletingLastPathComponent()]).snapshot(target: target)
        let evidence = observed.evidence(target: target, applicationRoots: [], hasStoreReceipt: false)
        XCTAssertEqual(evidence.homebrewReceiptClaims, [self.target])
        XCTAssertEqual(evidence.disposition, .otherManager)
        XCTAssertEqual(evidence.homebrewCoverageGaps, [NativeCaskCoverageGap(tokenPath: token.path, reason: .uninspectedArtifacts)])
        let encoded = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(evidence)) as? [String: Any])
        let gaps = try XCTUnwrap(encoded["homebrewCoverageGaps"] as? [[String: String]])
        XCTAssertEqual(gaps, [["tokenPath": token.path, "reason": "uninspectedArtifacts"]])
    }

    func testCoverageGapOrderIsDeterministicAcrossBothRoots() throws {
        let otherRoot = root.appendingPathComponent("OtherCaskroom", isDirectory: true)
        let otherToken = otherRoot.appendingPathComponent("second", isDirectory: true)
        try FileManager.default.createDirectory(at: otherToken, withIntermediateDirectories: true)
        let target = URL(fileURLWithPath: target, isDirectory: false)
        var results: [[NativeCaskCoverageGap]] = []
        for roots in [[otherRoot, token.deletingLastPathComponent()], [token.deletingLastPathComponent(), otherRoot]] {
            let observed = try NativeManagerObserver(caskrooms: roots).snapshot(target: target)
            results.append(observed.evidence(target: target, applicationRoots: [], hasStoreReceipt: false).homebrewCoverageGaps)
        }
        XCTAssertEqual(results[0], results[1])
        XCTAssertEqual(results[0].map(\.tokenPath), [token.path, otherToken.path].sorted())
        XCTAssertEqual(Set(results[0].map(\.reason)), [.missingReceipt, .missingMetadata])
    }

    func testAddingDeclarationsChangesGapAndSnapshotWithoutAddingClaims() throws {
        try install(json([:]))
        let before = try snapshot()
        try install(receipt(["Other.app"]))
        let after = try snapshot()
        XCTAssertEqual(before.claims, after.claims)
        XCTAssertEqual(before.coverageGap, .missingArtifactDeclarations)
        XCTAssertNil(after.coverageGap)
        XCTAssertNotEqual(before, after)
    }

    func testMissingOrUnsupportedSavedConfigurationFailsClosed() throws {
        for data in [nil, Data("[]".utf8), try json([:]), try json(["default": [:], "env": NSNull(), "explicit": [:]]),
                     try config("~/Applications"), try config("relative"), try config("/Applications", explicit: ["appdir": 7])] {
            XCTAssertThrowsError(try NativeCaskReceiptObserver.claims(receipt: receipt(), config: data, target: target))
        }
    }

    func testMalformedAppDeclarationsDoNotBecomeEmptySuccessfulScans() throws {
        for args: [Any] in [[], [7], [""], ["Example.app", "Elsewhere.app"], ["Example.app", ["wrong": "Example.app"]],
                            ["Example.app", ["target": 1]], ["Example.app", ["target": "~/Applications/Example.app"]],
                            ["Example.app", ["target": "Example.app"], "extra"], ["folder/"], [".."], ["Bad\n.app"]] {
            XCTAssertThrowsError(try claims(args))
        }
        for raw: Any in ["app", ["app": ["Example.app"]], [7], [["app": ["Example.app"], "pkg": ["x"]]]] {
            XCTAssertThrowsError(try NativeCaskReceiptObserver.claims(receipt: json(["uninstall_artifacts": raw]), config: config(), target: target))
        }
    }

    func testReceiptConfigAndArtifactBounds() throws {
        XCTAssertThrowsError(try NativeCaskReceiptObserver.claims(receipt: Data(repeating: 32, count: 256 * 1024 + 1), config: nil, target: target))
        XCTAssertThrowsError(try claims(configuration: Data(repeating: 32, count: 64 * 1024 + 1)))
        let data = try json(["uninstall_artifacts": Array(repeating: ["app": ["Example.app"]], count: 513)])
        XCTAssertThrowsError(try NativeCaskReceiptObserver.claims(receipt: data, config: config(), target: target))
        XCTAssertThrowsError(try claims([String(repeating: "x", count: 4097)]))
        try install()
        let bytes = try snapshot().byteCount
        XCTAssertEqual(try snapshot(budget: bytes).claims, [target])
        XCTAssertThrowsError(try snapshot(budget: bytes - 1))
    }

    func testRawReceiptOrConfigurationChangesInvalidateEqualClaims() throws {
        try install()
        let original = try snapshot()
        try install(json(["uninstall_artifacts": [["app": ["Example.app"]]], "time": 123]))
        let receiptChanged = try snapshot()
        XCTAssertEqual(original.claims, receiptChanged.claims)
        XCTAssertNotEqual(original, receiptChanged)
        try install(json(["uninstall_artifacts": [["app": ["Example.app"]]], "time": 123]),
                    configuration: config("/Applications", env: ["languages": ["en"]]))
        let configChanged = try snapshot()
        XCTAssertEqual(configChanged.claims, receiptChanged.claims)
        XCTAssertNotEqual(configChanged, receiptChanged)
    }

    func testAllTokensShareReceiptByteBudget() throws {
        let receipt = try json(["uninstall_artifacts": [], "padding": String(repeating: "x", count: 250 * 1024)])
        for index in 0..<17 {
            let directory = token.deletingLastPathComponent().appendingPathComponent("token-\(index)/.metadata", isDirectory: true)
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            try receipt.write(to: directory.appendingPathComponent("INSTALL_RECEIPT.json", isDirectory: false))
        }
        XCTAssertThrowsError(try NativeManagerObserver(caskrooms: [token.deletingLastPathComponent()]).snapshot(
            target: URL(fileURLWithPath: target, isDirectory: false))) {
            XCTAssertEqual($0 as? ObservationFailure, .limitExceeded)
        }
    }

    func testInvalidAndOversizedFilesFailClosed() throws {
        let path = metadata.appendingPathComponent("INSTALL_RECEIPT.json", isDirectory: false)
        for data in [Data(), Data("not json".utf8), Data("[]".utf8), Data(repeating: 32, count: 256 * 1024 + 1)] {
            try data.write(to: path)
            XCTAssertThrowsError(try snapshot())
        }
    }

    func testSymlinkedAndSpecialReceiptFilesRejectWithoutFollowingOrBlocking() throws {
        for name in ["INSTALL_RECEIPT.json", "config.json"] {
            let path = metadata.appendingPathComponent(name, isDirectory: false)
            try FileManager.default.createSymbolicLink(at: path, withDestinationURL: root.appendingPathComponent("absent", isDirectory: false))
            XCTAssertThrowsError(try snapshot())
            try FileManager.default.removeItem(at: path)
            XCTAssertEqual(mkfifo(path.path, 0o600), 0)
            XCTAssertThrowsError(try snapshot())
            try FileManager.default.removeItem(at: path)
        }
    }

    func testMissingConfigurationDoesNotHideReceiptAppClaim() throws {
        try receipt().write(to: metadata.appendingPathComponent("INSTALL_RECEIPT.json", isDirectory: false))
        XCTAssertThrowsError(try snapshot())
    }

    func testMetadataDirectoryAliasIsNotAnAbsentReceipt() throws {
        try FileManager.default.removeItem(at: metadata)
        try FileManager.default.createSymbolicLink(at: metadata, withDestinationURL: root.appendingPathComponent("absent", isDirectory: true))
        XCTAssertThrowsError(try snapshot())
    }
}
