import Foundation
import XCTest
@testable import HelmExternalUpdateObservation

final class ReceiptCatalogTests: XCTestCase {
    private let target = URL(fileURLWithPath: "/Applications/Example.app", isDirectory: true)
    private let id = "org.example.pkg"

    private func plist(_ value: Any) throws -> Data {
        try PropertyListSerialization.data(fromPropertyList: value, format: .xml, options: 0)
    }

    private func info(_ location: String) -> [String: Any] {
        ["pkgid": id, "volume": "/", "install-location": location, "install-time": 123, "pkg-version": "1"]
    }

    private func catalog(location: String, paths: [String], mutate: ((inout [String: Any]) -> Void)? = nil,
                         calls: @escaping ([String]) -> Void = { _ in }) -> NativeReceiptCatalog {
        NativeReceiptCatalog { args, _, _ in
            calls(args)
            switch args[0] {
            case "--pkgs-plist": return try self.plist([self.id])
            case "--pkg-info-plist": return try self.plist(self.info(location))
            case "--files": return Data(paths.map { $0 + "\n" }.joined().utf8)
            case "--export-plist":
                var value = self.info(location)
                value["paths"] = Dictionary(uniqueKeysWithValues: paths.map { ($0, ["pkgid": self.id]) })
                mutate?(&value)
                return try self.plist(value)
            default: XCTFail("Unexpected system query"); throw ObservationFailure.unreadableManagerEvidence
            }
        }
    }

    private func snapshot(_ observer: NativeReceiptCatalog) throws -> NativeInstallerReceiptObserver.Snapshot {
        try observer.snapshot(target: target, remaining: { 3_000_000_000 })
    }

    func testNonRootResourceLocationResolvesRelativePayload() throws {
        let result = try snapshot(catalog(location: "Applications/Example.app/Contents/Resources", paths: ["Icon.icns"]))
        XCTAssertEqual(result.identifiers, [id])
        XCTAssertEqual(result.replies.count, 3)
    }

    func testAncestorInstallLocationResolvesTargetOnComponentBoundary() throws {
        XCTAssertEqual(try snapshot(catalog(location: "/Applications", paths: ["./Example.app/Contents/Resources/Icon"])).identifiers, [id])
        XCTAssertTrue(try snapshot(catalog(location: "/Applications", paths: ["Example.app.other/Contents/Icon"])).identifiers.isEmpty)
        XCTAssertTrue(try snapshot(catalog(location: "/Applications", paths: ["Other.app/Contents/Icon"])).identifiers.isEmpty)
    }

    func testCaseEquivalentInstallLocationsRemainExclusions() throws {
        for location in ["/applications/example.app/Contents/Resources", "/APPLICATIONS/EXAMPLE.APP"] {
            XCTAssertEqual(try snapshot(catalog(location: location, paths: ["Icon.icns"])).identifiers, [id])
        }
    }

    func testCaseEquivalentPayloadPathsRemainExclusionsWithoutMatchingSiblings() throws {
        XCTAssertEqual(try snapshot(catalog(location: "/Applications", paths: ["example.APP/Contents/Icon.icns"])).identifiers, [id])
        XCTAssertTrue(try snapshot(catalog(location: "/applications", paths: ["EXAMPLE.APP.other/Contents/Icon.icns"])).identifiers.isEmpty)
        XCTAssertTrue(try snapshot(catalog(location: "/applications/EXAMPLE.APP.other", paths: ["Icon.icns"])).identifiers.isEmpty)
    }

    func testCanonicalUnicodeAndCaseEquivalentReceiptPathsRemainExclusions() throws {
        let unicodeTarget = URL(fileURLWithPath: "/Applications/Caf\u{E9}.app", isDirectory: true)
        let observer = catalog(location: "/applications", paths: ["CAFE\u{301}.APP/Contents/Icon.icns"])
        XCTAssertEqual(try observer.snapshot(target: unicodeTarget, remaining: { 3_000_000_000 }).identifiers, [id])
    }

    func testCaseOnlyReceiptDriftStillChangesTheRawSnapshot() throws {
        let first = try snapshot(catalog(location: target.path, paths: ["Contents/Icon.icns"]))
        let second = try snapshot(catalog(location: target.path, paths: ["CONTENTS/ICON.ICNS"]))
        XCTAssertEqual(first.identifiers, second.identifiers)
        XCTAssertNotEqual(first, second)
    }

    func testCaseOnlyExportLocationDriftStillRejects() throws {
        XCTAssertThrowsError(try snapshot(catalog(location: target.path, paths: ["Icon.icns"], mutate: {
            $0["install-location"] = self.target.path.lowercased()
        })))
    }

    func testCaseEquivalentCatalogClaimSurvivesEmptyFileInfo() throws {
        let observer = NativeInstallerReceiptObserver(batchQuery: { paths, _ in
            try paths.reduce(into: Data()) { $0.append(try ReceiptFixtures.reply(path: $1)) }
        }, catalog: catalog(location: "/applications/example.app/Contents/Resources", paths: ["Icon.icns"]))
        XCTAssertEqual(try observer.snapshot(target: target, paths: [target.path]).identifiers, [id])
    }

    func testTargetRootReceiptAndHistoricalPayloadAreConservativeClaims() throws {
        XCTAssertEqual(try snapshot(catalog(location: target.path, paths: ["."])).identifiers, [id])
        XCTAssertEqual(try snapshot(catalog(location: target.path, paths: ["Contents/Deleted.dat"])).identifiers, [id])
    }

    func testRootLocationsUseListingsAndNonOverlappingLocationsDoNotExport() throws {
        for location in ["", "/", "/Library", "/Applications/Other.app", target.path + ".other"] {
            var exports = 0
            var listings = 0
            XCTAssertTrue(try snapshot(catalog(location: location, paths: [], calls: { args in
                if args[0] == "--export-plist" { exports += 1 }
                if args[0] == "--files" { listings += 1 }
            })).identifiers.isEmpty)
            XCTAssertEqual(exports, 0)
            XCTAssertEqual(listings, ["", "/"].contains(location) ? 1 : 0)
        }
    }

    func testRootHistoricalPayloadClaimsSurviveEmptyFileInfo() throws {
        for location in ["", "/", "."] {
            let observer = NativeInstallerReceiptObserver(batchQuery: { paths, _ in
                try paths.reduce(into: Data()) { $0.append(try ReceiptFixtures.reply(path: $1)) }
            }, catalog: catalog(location: location, paths: ["Applications/Example.app/Contents/Deleted.dat"]))
            let result = try observer.snapshot(target: target, paths: [target.path])
            XCTAssertEqual(result.identifiers, [id])
            XCTAssertEqual(result.rootPayloads.count, 1)
            XCTAssertTrue(try XCTUnwrap(result.rootPayloads.first).claimsTarget)
        }
    }

    func testRootPayloadChangesBindEvenWithoutClaims() throws {
        let first = try snapshot(catalog(location: "/", paths: ["Library/Old.dat"]))
        let second = try snapshot(catalog(location: "/", paths: ["Library/New.dat"]))
        XCTAssertEqual(first.replies, second.replies)
        XCTAssertEqual(first.identifiers, second.identifiers)
        XCTAssertNotEqual(first, second)
    }

    func testRootListingFailureAndMalformedReplyDoNotBecomeEmptyEvidence() throws {
        for reply in [Data("Applications/Example.app".utf8), Data([0xFF, 10]), Data(repeating: 65, count: NativeRootReceiptPayload.maximumBytes + 1)] {
            let observer = NativeReceiptCatalog { args, _, _ in
                switch args[0] {
                case "--pkgs-plist": return try self.plist([self.id])
                case "--pkg-info-plist": return try self.plist(self.info("/"))
                case "--files": return reply
                default: throw ObservationFailure.unreadableManagerEvidence
                }
            }
            XCTAssertThrowsError(try snapshot(observer))
        }
        let observer = NativeReceiptCatalog { args, _, _ in
            if args[0] == "--pkgs-plist" { return try self.plist([self.id]) }
            if args[0] == "--pkg-info-plist" { return try self.plist(self.info("/")) }
            throw ObservationFailure.unreadableManagerEvidence
        }
        XCTAssertThrowsError(try snapshot(observer))
    }

    func testRootListingsShareAggregateBudgetWithoutRelaxingExistingLimits() throws {
        let identifiers = (0..<5).map { "org.example.pkg\($0)" }
        let listing = Data(String(repeating: String(repeating: "a", count: 4095) + "\n", count: 2048).utf8)
        XCTAssertEqual(listing.count, NativeRootReceiptPayload.maximumBytes)
        var calls = 0
        let observer = NativeReceiptCatalog { args, _, limit in
            switch args[0] {
            case "--pkgs-plist": return try self.plist(identifiers)
            case "--pkg-info-plist":
                return try identifiers.reduce(into: Data()) { result, id in
                    var info = self.info("/")
                    info["pkgid"] = id
                    result.append(try self.plist(info))
                }
            case "--files":
                calls += 1
                XCTAssertEqual(limit, NativeRootReceiptPayload.maximumBytes)
                return listing
            default: throw ObservationFailure.unreadableManagerEvidence
            }
        }
        XCTAssertThrowsError(try snapshot(observer)) { XCTAssertEqual($0 as? ObservationFailure, .limitExceeded) }
        XCTAssertEqual(calls, 4)
        XCTAssertEqual(NativeReceiptCatalog.maximumBytes, 4 * 1024 * 1024)
        XCTAssertEqual(NativeInstallerReceiptObserver.byteLimit, 4 * 1024 * 1024)
    }

    func testRootQueryConsumesTheSharedSnapshotDeadline() throws {
        var now: UInt64 = 0
        var fileInfoCalls = 0
        let catalog = catalog(location: "/", paths: ["Applications/Example.app"], calls: { args in
            if args[0] == "--files" { now = NativeInstallerReceiptObserver.snapshotNanoseconds }
        })
        let observer = NativeInstallerReceiptObserver(batchQuery: { _, _ in
            fileInfoCalls += 1
            return Data()
        }, clock: { now }, catalog: catalog)
        XCTAssertThrowsError(try observer.snapshot(target: target, paths: [target.path])) {
            XCTAssertEqual($0 as? ObservationFailure, .limitExceeded)
        }
        XCTAssertEqual(fileInfoCalls, 0)
    }

    func testMissingMalformedOrUnsafeCatalogDoesNotMeanNoReceipts() throws {
        for value: Any in [["--forget"], ["-option"], [" leading"], ["bad\nname"], [id, id], [""], [1], [:],
                          [String(repeating: "x", count: 256)], (0..<1025).map { "org.pkg.\($0)" }] {
            var calls = 0
            let observer = NativeReceiptCatalog { _, _, _ in calls += 1; return try self.plist(value) }
            XCTAssertThrowsError(try snapshot(observer))
            XCTAssertEqual(calls, 1)
        }
    }

    func testQueryErrorAndOversizedOutputAreNotPartialCoverage() throws {
        XCTAssertThrowsError(try snapshot(NativeReceiptCatalog { _, _, _ in throw ObservationFailure.unreadableManagerEvidence }))
        XCTAssertThrowsError(try snapshot(NativeReceiptCatalog { _, _, limit in Data(repeating: 32, count: limit + 1) }))
    }

    func testMetadataIsBoundedStrictlyBoundToIDAndRootVolume() throws {
        for key in ["pkgid", "volume", "install-location", "pkg-version", "install-time"] {
            let observer = NativeReceiptCatalog { args, _, _ in
                if args[0] == "--pkgs-plist" { return try self.plist([self.id]) }
                var value = self.info("Applications")
                value.removeValue(forKey: key)
                return try self.plist(value)
            }
            XCTAssertThrowsError(try snapshot(observer))
        }
        for (key, value) in [("pkgid", "other.pkg"), ("volume", "/Volumes/Other"), ("install-location", "../Applications")] {
            let observer = NativeReceiptCatalog { args, _, _ in
                if args[0] == "--pkgs-plist" { return try self.plist([self.id]) }
                var record = self.info("Applications")
                record[key] = value
                return try self.plist(record)
            }
            XCTAssertThrowsError(try snapshot(observer))
        }
    }

    func testExportDriftAndMalformedPayloadReject() throws {
        let mutations: [(inout [String: Any]) -> Void] = [
            { $0["install-location"] = "/Other" }, { $0["pkg-version"] = "2" }, { $0["install-time"] = 124 },
            { $0["paths"] = [] }, { $0["paths"] = ["Icon": ["pkgid": "other.pkg"]] }, { $0["paths"] = ["Icon": "bad"] }
        ]
        for mutate in mutations {
            XCTAssertThrowsError(try snapshot(catalog(location: target.path, paths: ["Icon"], mutate: mutate)))
        }
        for path in ["../Outside", "/Absolute", "Line\nBreak", String(repeating: "a", count: 4097)] {
            XCTAssertThrowsError(try snapshot(catalog(location: target.path, paths: [path])))
        }
    }

    func testDeadlineAppliesBeforeAndAfterEveryCatalogQuery() throws {
        var admitted = true
        var calls = 0
        let observer = NativeReceiptCatalog { _, _, _ in
            calls += 1
            admitted = false
            return try self.plist([String]())
        }
        XCTAssertThrowsError(try observer.snapshot(target: target, remaining: {
            guard admitted else { throw ObservationFailure.limitExceeded }
            return 1
        }))
        XCTAssertEqual(calls, 1)
    }

    func testCatalogDriftChangesSnapshotWithSameClaim() throws {
        let first = try snapshot(catalog(location: target.path, paths: ["Icon"]))
        let second = try snapshot(catalog(location: target.path, paths: ["Icon"], mutate: { $0["new-metadata"] = 1 }))
        XCTAssertEqual(first.identifiers, second.identifiers)
        XCTAssertNotEqual(first, second)
    }

    func testNativeCatalogAndFileInfoClaimsAreCombined() throws {
        let observer = NativeInstallerReceiptObserver(batchQuery: { paths, _ in
            try paths.reduce(into: Data()) { $0.append(try ReceiptFixtures.reply(path: $1, identifiers: ["org.other.pkg"])) }
        }, catalog: catalog(location: target.path, paths: ["Icon"]))
        XCTAssertEqual(try observer.snapshot(target: target, paths: [target.path]).identifiers, [id, "org.other.pkg"])
    }
}
