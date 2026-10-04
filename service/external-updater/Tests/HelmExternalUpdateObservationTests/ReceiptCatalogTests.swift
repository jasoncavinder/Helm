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

    func testTargetRootReceiptAndHistoricalPayloadAreConservativeClaims() throws {
        XCTAssertEqual(try snapshot(catalog(location: target.path, paths: ["."])).identifiers, [id])
        XCTAssertEqual(try snapshot(catalog(location: target.path, paths: ["Contents/Deleted.dat"])).identifiers, [id])
    }

    func testRootAndNonOverlappingLocationsDoNotExport() throws {
        for location in ["", "/", "/Library", "/Applications/Other.app", target.path + ".other"] {
            var exports = 0
            XCTAssertTrue(try snapshot(catalog(location: location, paths: [], calls: { args in
                if args[0] == "--export-plist" { exports += 1 }
            })).identifiers.isEmpty)
            XCTAssertEqual(exports, 0)
        }
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
