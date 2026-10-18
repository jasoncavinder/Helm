import Darwin
import Foundation
import XCTest
@testable import HelmExternalUpdateObservation

enum OwnershipFixtures {
    // Synthetic source snapshots exercise binding only, never native clearance.
    static func fingerprint(receipt: String = "reviewed") -> NativeOwnershipFingerprint? {
        try? NativeOwnershipFingerprint(target: URL(fileURLWithPath: "/Applications/Example.app"),
            applicationRoots: [URL(fileURLWithPath: "/Applications")], caskrooms: NativeManagerObserver.defaultCaskrooms,
            macportsRegistry: NativeMacPortsObserver.registry, bundleEntries: [:], homebrew: .init(),
            installer: .init(replies: [Data(receipt.utf8)], identifiers: []),
            macports: .init(filesystem: [:], reply: Data(), claims: []))
    }
}

final class OwnershipFingerprintTests: XCTestCase {
    private func fingerprint(homebrew: NativeManagerObserver.Snapshot = .init(),
                             installer: NativeInstallerReceiptObserver.Snapshot = .init(replies: [], identifiers: []),
                             macports: NativeMacPortsObserver.Snapshot = .init(filesystem: [:], reply: Data(), claims: []),
                             entries: [String: FileIdentity] = [:], target: String = "/Applications/Example.app",
                             roots: [String] = ["/Applications"], caskrooms: [String] = ["/opt/homebrew/Caskroom"],
                             registry: String = NativeMacPortsObserver.registry.path) throws -> NativeOwnershipFingerprint {
        try NativeOwnershipFingerprint(target: URL(fileURLWithPath: target),
            applicationRoots: roots.map { URL(fileURLWithPath: $0, isDirectory: true) },
            caskrooms: caskrooms.map { URL(fileURLWithPath: $0, isDirectory: true) },
            macportsRegistry: URL(fileURLWithPath: registry, isDirectory: false), bundleEntries: entries,
            homebrew: homebrew, installer: installer, macports: macports)
    }

    func testRawCaskReceiptAndConfigurationDriftEvenWithoutClaims() throws {
        let token = "/opt/homebrew/Caskroom/unrelated"
        var snapshot = NativeManagerObserver.Snapshot()
        snapshot.receipts[token] = .init(bytes: ["INSTALL_RECEIPT.json": Data("{}".utf8), "config.json": Data("{}".utf8)],
                                        coverageGap: nil)
        let initial = try fingerprint(homebrew: snapshot)
        for file in ["INSTALL_RECEIPT.json", "config.json"] {
            var changed = snapshot
            changed.receipts[token]?.bytes[file] = Data("{ }".utf8)
            XCTAssertEqual(changed.receipts[token]?.claims, snapshot.receipts[token]?.claims)
            XCTAssertNotEqual(try fingerprint(homebrew: changed), initial)
        }
    }

    func testRawInstallerResponsesAndMessageBoundariesAreBound() throws {
        let first = NativeInstallerReceiptObserver.Snapshot(replies: [Data("ab".utf8), Data("c".utf8)], identifiers: [])
        for replies in [[Data("a".utf8), Data("bc".utf8)], [Data("ab ".utf8), Data("c".utf8)], first.replies.reversed().map { $0 }] {
            XCTAssertNotEqual(try fingerprint(installer: first), try fingerprint(installer: .init(replies: replies, identifiers: [])))
        }
        XCTAssertNotEqual(try fingerprint(installer: first), try fingerprint(installer: .init(replies: first.replies, identifiers: ["pkg"])))
    }

    func testMacPortsRawReplyAndRegistryIdentityAreBoundWithoutClaims() throws {
        var status = stat()
        status.st_ino = 1
        let first = NativeMacPortsObserver.Snapshot(filesystem: ["/registry": FileIdentity(status)], reply: Data("[]".utf8), claims: [])
        XCTAssertNotEqual(try fingerprint(macports: first), try fingerprint(macports: .init(filesystem: first.filesystem,
            reply: Data("[ ]".utf8), claims: [])))
        status.st_ino = 2
        XCTAssertNotEqual(try fingerprint(macports: first), try fingerprint(macports: .init(filesystem: ["/registry": FileIdentity(status)],
            reply: first.reply, claims: [])))
    }

    func testCaskRootAbsenceLinkDestinationAndIdentityAreBound() throws {
        let baseline = try fingerprint()
        var snapshot = NativeManagerObserver.Snapshot()
        snapshot.absentRoots = ["/usr/local/Caskroom"]
        XCTAssertNotEqual(try fingerprint(homebrew: snapshot), baseline)
        snapshot = .init()
        snapshot.links["/cask/ref"] = "/Applications/Unrelated.app"
        let first = try fingerprint(homebrew: snapshot)
        snapshot.links["/cask/ref"] = "/Applications/Other.app"
        XCTAssertNotEqual(try fingerprint(homebrew: snapshot), first)
        var status = stat()
        status.st_ino = 1
        snapshot.identities["/cask"] = FileIdentity(status)
        let withIdentity = try fingerprint(homebrew: snapshot)
        status.st_ctimespec.tv_nsec = 1
        snapshot.identities["/cask"] = FileIdentity(status)
        XCTAssertNotEqual(try fingerprint(homebrew: snapshot), withIdentity)
    }

    func testTargetScopeAndInspectedBundleEntriesAreBound() throws {
        let baseline = try fingerprint()
        XCTAssertNotEqual(baseline, try fingerprint(target: "/Applications/Other.app"))
        XCTAssertNotEqual(baseline, try fingerprint(roots: ["/Applications", "/Users/agent/Applications"]))
        XCTAssertNotEqual(baseline, try fingerprint(caskrooms: ["/usr/local/Caskroom"]))
        XCTAssertNotEqual(baseline, try fingerprint(registry: "/another/registry.db"))
        XCTAssertNotEqual(baseline, try fingerprint(entries: ["/Applications/Example.app": FileIdentity(stat())]))
    }

    func testDictionaryInsertionOrderDoesNotChangeEvidence() throws {
        let paths = ["/cask/Z", "/cask/a", "/cask/A", "/cask/cafe", "/cask/caf\u{e9}", "/cask/2", "/cask/10"]
        var first = NativeManagerObserver.Snapshot()
        var second = NativeManagerObserver.Snapshot()
        for path in paths { first.links[path] = path }
        for path in paths.reversed() { second.links[path] = path }
        XCTAssertEqual(try fingerprint(homebrew: first), try fingerprint(homebrew: second))
    }

    func testFingerprintIsInternalAndNeverGrantsAuthority() throws {
        let first = try XCTUnwrap(OwnershipFixtures.fingerprint())
        let second = try XCTUnwrap(OwnershipFixtures.fingerprint(receipt: "changed"))
        let original = NativeManagerEvidence(exclusions: [], homebrewReferences: [], inspectedCaskEntries: 0, observationFingerprint: first)
        let changed = NativeManagerEvidence(exclusions: [], homebrewReferences: [], inspectedCaskEntries: 0, observationFingerprint: second)
        XCTAssertNotEqual(original, changed)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        XCTAssertEqual(try encoder.encode(original), try encoder.encode(changed))
        XCTAssertEqual(original.disposition, .unresolved)
        XCTAssertTrue(original.homebrewCoverageGaps.isEmpty)
    }
}
