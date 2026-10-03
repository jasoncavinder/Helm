import Darwin
import Foundation
import XCTest
@testable import HelmExternalUpdateObservation

final class ManagerObservationTests: XCTestCase {
    private var root: URL!
    private var apps: URL!
    private var target: URL!
    private var caskroom: URL!
    private var reference: URL!

    override func setUpWithError() throws {
        root = FileManager.default.homeDirectoryForCurrentUser.resolvingSymlinksInPath()
            .appendingPathComponent("helm-manager-observer-\(UUID().uuidString)")
        apps = root.appendingPathComponent("Applications")
        target = apps.appendingPathComponent("Example.app")
        caskroom = root.appendingPathComponent("Caskroom")
        reference = caskroom.appendingPathComponent("example/100/Example.app")
        let resources = target.appendingPathComponent("Contents/Frameworks/Sparkle.framework/Resources")
        try FileManager.default.createDirectory(at: resources, withIntermediateDirectories: true)
        XCTAssertEqual(chmod(root.path, 0o700), 0)
        try plist(signature().info).write(to: target.appendingPathComponent("Contents/Info.plist"))
        try plist(["CFBundleIdentifier": "org.sparkle-project.Sparkle", "CFBundleShortVersionString": "2.9.5"])
            .write(to: resources.appendingPathComponent("Info.plist"))
    }

    override func tearDownWithError() throws {
        if let root { try FileManager.default.removeItem(at: root) }
    }

    private func plist(_ info: [String: Any]) throws -> Data {
        try PropertyListSerialization.data(fromPropertyList: info, format: .xml, options: 0)
    }

    private func signature() -> NativeSigningEvidence {
        NativeSigningEvidence(identifier: "org.example.App", team: "ABCDE12345", hash: Data(repeating: 1, count: 20), info: [
            "CFBundleIdentifier": "org.example.App", "CFBundleVersion": "100", "SUFeedURL": "https://example.org/feed",
            "SUPublicEDKey": Data(repeating: 7, count: 32).base64EncodedString()
        ])
    }

    private func link(_ destination: String? = nil, at path: URL? = nil) throws {
        let path = path ?? reference!
        try FileManager.default.createDirectory(at: path.deletingLastPathComponent(), withIntermediateDirectories: true)
        try FileManager.default.createSymbolicLink(atPath: path.path, withDestinationPath: destination ?? target.path)
    }

    private func scanner(limit: Int = 10_000) -> NativeManagerObserver {
        NativeManagerObserver(caskrooms: [caskroom], entryLimit: limit)
    }

    private func observe(_ mutation: (() throws -> Void)? = nil) throws -> NativeTargetEvidence {
        try NativeTargetObserver(roots: [apps], managers: scanner()) { _ in
            try mutation?()
            return self.signature()
        }.observe(path: target.path)
    }

    func testMissingRootNeverEstablishesStandalone() throws {
        let evidence = try observe()
        XCTAssertEqual(evidence.managerEvidence.disposition, .unresolved)
        XCTAssertTrue(evidence.managerEvidence.exclusions.isEmpty)
        XCTAssertTrue(evidence.requiresAuthorityResolution)
    }

    func testEmptyRootNeverEstablishesStandalone() throws {
        try FileManager.default.createDirectory(at: caskroom, withIntermediateDirectories: true)
        XCTAssertEqual(try observe().managerEvidence.disposition, .unresolved)
    }

    func testExactHomebrewReferenceExcludesTarget() throws {
        try link()
        let evidence = try observe()
        XCTAssertEqual(evidence.managerEvidence.disposition, .otherManager)
        XCTAssertEqual(evidence.managerEvidence.exclusions, [.homebrewCaskReference])
        XCTAssertEqual(evidence.managerEvidence.homebrewReferences, [reference.path])
        XCTAssertEqual(evidence.managerEvidence.inspectedCaskEntries, 3)
        XCTAssertTrue(evidence.requiresAuthorityResolution)
    }

    func testRelativeReferenceAndRenamedArtifactStillMatchExactPath() throws {
        reference = caskroom.appendingPathComponent("unrelated-token/100/Old Name.app")
        try link("../../../Applications/Example.app")
        XCTAssertEqual(try observe().managerEvidence.disposition, .otherManager)
    }

    func testSameNameInAnotherApplicationDirectoryDoesNotMatch() throws {
        try link(root.appendingPathComponent("other/Example.app").path)
        XCTAssertEqual(try observe().managerEvidence.disposition, .unresolved)
    }

    func testLexicalReferenceNormalization() {
        let directory = URL(fileURLWithPath: "/prefix/Caskroom/example/100", isDirectory: true)
        let cases = [
            ("/Applications/./Nested/../Example.app/", "/Applications/Example.app"),
            ("../../../../Applications//Example.app", "/Applications/Example.app"),
            ("../../../../../../Applications/Example.app", "/Applications/Example.app"),
            ("./Example.app", "/prefix/Caskroom/example/100/Example.app"),
            ("/../Applications/Example.app", "/Applications/Example.app"),
            ("/Volumes/offline/../Example.app", "/Volumes/Example.app"),
            ("~/Example.app", "/prefix/Caskroom/example/100/~/Example.app"),
            ("/Applications/100%20日本.app", "/Applications/100%20日本.app")
        ]
        for (destination, expected) in cases {
            XCTAssertEqual(NativeManagerObserver.lexicalReferenceURL(destination, relativeTo: directory).path, expected)
        }
    }

    func testReferenceURLNeverInfersDestinationDirectoryStatus() throws {
        let destination = root.appendingPathComponent("destination", isDirectory: false)
        for exists in [false, true] {
            if exists { try FileManager.default.createDirectory(at: destination, withIntermediateDirectories: true) }
            for text in [destination.path, "destination"] {
                let resolved = NativeManagerObserver.lexicalReferenceURL(text, relativeTo: root)
                XCTAssertEqual(resolved.path, destination.path)
                // The directory flag must be an explicit inert hint, not a
                // fact inferred by probing the destination's filesystem.
                XCTAssertFalse(resolved.hasDirectoryPath, "Destination exists: \(exists), link text: \(text)")
            }
        }
    }

    func testAbsoluteReferenceWithDotComponentsStillMatches() throws {
        try link(apps.path + "/./Unrelated/../Example.app")
        XCTAssertEqual(try observe().managerEvidence.homebrewReferences, [reference.path])
    }

    func testRelativeDestinationAliasRemainsUnresolved() throws {
        let alias = root.appendingPathComponent("Alias.app", isDirectory: false)
        try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: target)
        try link("../../../Alias.app")
        XCTAssertEqual(try observe().managerEvidence.disposition, .unresolved)
    }

    func testMultiplePrefixesAndClaimsAreRetainedInSortedOrder() throws {
        try link()
        let second = root.appendingPathComponent("other/Caskroom")
        let secondLink = second.appendingPathComponent("example/200/Example.app")
        try link(at: secondLink)
        let snapshot = try NativeManagerObserver(caskrooms: [second, caskroom]).snapshot(target: target)
        XCTAssertEqual(snapshot.references, [reference.path, secondLink.path].sorted())
        XCTAssertEqual(snapshot.entryCount, 6)
    }

    func testGroupWritableCaskroomCanOnlySupplyExclusions() throws {
        try link()
        XCTAssertEqual(chmod(caskroom.path, 0o775), 0)
        XCTAssertEqual(try observe().managerEvidence.disposition, .otherManager)
    }

    func testConcreteAppAndMetadataAreNotEvaluatedOrTraversed() throws {
        try FileManager.default.createDirectory(at: reference, withIntermediateDirectories: true)
        let metadata = caskroom.appendingPathComponent("example/.metadata")
        try FileManager.default.createDirectory(at: metadata, withIntermediateDirectories: true)
        XCTAssertEqual(mkfifo(metadata.appendingPathComponent("malicious.rb").path, 0o600), 0)
        XCTAssertEqual(mkfifo(reference.appendingPathComponent("do-not-open").path, 0o600), 0)
        XCTAssertEqual(try observe().managerEvidence.disposition, .unresolved)
    }

    func testDestinationAliasesAreNeverFollowedToInventOwnership() throws {
        let alias = root.appendingPathComponent("Alias.app")
        try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: target)
        try link(alias.path)
        XCTAssertEqual(try observe().managerEvidence.disposition, .unresolved)
    }

    func testSymlinkedRootIsRejectedNotReportedAsMissing() throws {
        try FileManager.default.createSymbolicLink(at: caskroom, withDestinationURL: root.appendingPathComponent("missing"))
        XCTAssertThrowsError(try observe())
    }

    func testSymlinkedTokenAndVersionDirectoriesFailClosed() throws {
        try FileManager.default.createDirectory(at: caskroom, withIntermediateDirectories: true)
        let token = caskroom.appendingPathComponent("example")
        try FileManager.default.createSymbolicLink(at: token, withDestinationURL: root)
        XCTAssertThrowsError(try observe())
        try FileManager.default.removeItem(at: token)
        try FileManager.default.createDirectory(at: token, withIntermediateDirectories: true)
        try FileManager.default.createSymbolicLink(at: token.appendingPathComponent("100"), withDestinationURL: root)
        XCTAssertThrowsError(try observe())
    }

    func testIntermediateRootAliasFailsClosed() throws {
        let alias = root.appendingPathComponent("alias")
        try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: root)
        XCTAssertThrowsError(try NativeManagerObserver(caskrooms: [alias.appendingPathComponent("missing/Caskroom")]).snapshot(target: target))
    }

    func testNonDirectoryRootAndUnreadableRootFailClosed() throws {
        try Data("not a directory".utf8).write(to: caskroom)
        XCTAssertThrowsError(try observe())
        try FileManager.default.removeItem(at: caskroom)
        try FileManager.default.createDirectory(at: caskroom, withIntermediateDirectories: true)
        XCTAssertEqual(chmod(caskroom.path, 0o000), 0)
        defer { _ = chmod(caskroom.path, 0o700) }
        XCTAssertThrowsError(try observe())
    }

    func testBoundCountsEvenIgnoredEntriesAndAllPrefixes() throws {
        try link()
        XCTAssertThrowsError(try scanner(limit: 2).snapshot(target: target)) { error in
            XCTAssertEqual(error as? ObservationFailure, .limitExceeded)
        }
        try Data().write(to: caskroom.appendingPathComponent(".ignored"))
        XCTAssertThrowsError(try scanner(limit: 3).snapshot(target: target))
        XCTAssertThrowsError(try NativeManagerObserver(caskrooms: [caskroom, caskroom], entryLimit: 7).snapshot(target: target))
    }

    func testMalformedLinkAndEntryNamesFailClosed() throws {
        try link("/Applications/Example.app\n")
        XCTAssertThrowsError(try observe())
        try FileManager.default.removeItem(at: reference)
        try link(at: reference.deletingLastPathComponent().appendingPathComponent("invalid\n.app"))
        XCTAssertThrowsError(try observe())
    }

    func testReceiptAndCaskExclusionsAreNotMutuallyExclusive() throws {
        try link()
        let receipt = target.appendingPathComponent("Contents/_MASReceipt/receipt")
        try FileManager.default.createDirectory(at: receipt.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("not interpreted or validated".utf8).write(to: receipt)
        let evidence = try observe()
        XCTAssertTrue(evidence.hasStoreReceipt)
        XCTAssertEqual(evidence.managerEvidence.exclusions, [.appStoreReceipt, .homebrewCaskReference])
    }

    func testSetappLocationIsComponentBounded() throws {
        for (folder, expected) in [("Setapp", true), ("Setapp-like", false)] {
            let parent = apps.appendingPathComponent(folder)
            try FileManager.default.createDirectory(at: parent, withIntermediateDirectories: true)
            let moved = parent.appendingPathComponent("Example.app")
            try FileManager.default.moveItem(at: target, to: moved)
            target = moved
            XCTAssertEqual(try observe().managerEvidence.exclusions.contains(.setappLocation), expected)
        }
    }

    func testReceiptContainerAliasAndEmptyContainerRemainExclusions() throws {
        let container = target.appendingPathComponent("Contents/_MASReceipt")
        let actual = target.appendingPathComponent("Contents/Receipts")
        try FileManager.default.createDirectory(at: actual, withIntermediateDirectories: true)
        try Data("receipt".utf8).write(to: actual.appendingPathComponent("receipt"))
        try FileManager.default.createSymbolicLink(at: container, withDestinationURL: actual)
        XCTAssertEqual(try observe().managerEvidence.exclusions, [.appStoreReceipt])
        try FileManager.default.removeItem(at: container)
        try FileManager.default.createDirectory(at: container, withIntermediateDirectories: true)
        XCTAssertEqual(try observe().managerEvidence.exclusions, [.appStoreReceipt])
    }

    func testNewClaimDuringSignatureValidationFailsClosed() throws {
        XCTAssertThrowsError(try observe { try self.link() }) { error in
            XCTAssertEqual(error as? ObservationFailure, .changedDuringObservation)
        }
    }

    func testRemovedClaimDuringSignatureValidationFailsClosed() throws {
        try link()
        XCTAssertThrowsError(try observe { try FileManager.default.removeItem(at: self.reference) }) { error in
            XCTAssertEqual(error as? ObservationFailure, .changedDuringObservation)
        }
    }

    func testChangedLinkDuringSignatureValidationFailsClosed() throws {
        try link()
        XCTAssertThrowsError(try observe {
            try FileManager.default.removeItem(at: self.reference)
            try self.link(self.apps.appendingPathComponent("Other.app").path)
        }) { error in
            XCTAssertEqual(error as? ObservationFailure, .changedDuringObservation)
        }
    }

    func testNewReceiptDuringSignatureValidationFailsClosed() throws {
        XCTAssertThrowsError(try observe {
            let receipt = self.target.appendingPathComponent("Contents/_MASReceipt/receipt")
            try FileManager.default.createDirectory(at: receipt.deletingLastPathComponent(), withIntermediateDirectories: true)
            try Data("receipt".utf8).write(to: receipt)
        }) { error in
            XCTAssertEqual(error as? ObservationFailure, .changedDuringObservation)
        }
    }

    func testChangedCaskDirectoryPermissionsInvalidateSnapshot() throws {
        try link()
        XCTAssertThrowsError(try observe { XCTAssertEqual(chmod(self.caskroom.path, 0o700), 0) }) { error in
            XCTAssertEqual(error as? ObservationFailure, .changedDuringObservation)
        }
    }

    func testEvidenceEncodingIncludesUnresolvedDisposition() throws {
        let data = try JSONEncoder().encode(observe().managerEvidence)
        let json = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
        XCTAssertEqual(json["disposition"] as? String, "unresolved")
        XCTAssertEqual(NativeManagerObserver.defaultCaskrooms.map(\.path), ["/opt/homebrew/Caskroom", "/usr/local/Caskroom"])
    }
}
