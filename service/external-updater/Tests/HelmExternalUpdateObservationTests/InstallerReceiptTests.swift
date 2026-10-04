import Darwin
import Foundation
import XCTest
@testable import HelmExternalUpdateObservation

final class InstallerReceiptTests: XCTestCase {
    private let target = URL(fileURLWithPath: "/Applications/Space & Quote's.app", isDirectory: true)
    private var paths: [String] {
        [target.path, target.path + "/Contents/Info.plist", target.path + "/Contents/MacOS/Main",
         target.path + "/Contents/Resources/Payload.dat"]
    }

    func testEmptyResponsesRemainOnlyAbsenceOfTheseMarkers() throws {
        let snapshot = try ReceiptFixtures.empty.snapshot(target: target, paths: paths)
        XCTAssertEqual(snapshot.replies.count, 2)
        XCTAssertTrue(snapshot.identifiers.isEmpty)
    }

    func testEveryInspectedPathIsQueriedInDeterministicOrder() throws {
        var queried: [String] = []
        let observer = NativeInstallerReceiptObserver { path in
            queried.append(path)
            return try ReceiptFixtures.reply(path: path, identifiers: path.hasSuffix("/Payload.dat") ? ["org.example.installer"] : [])
        }
        XCTAssertEqual(try observer.snapshot(target: target, paths: paths.reversed()).identifiers, ["org.example.installer"])
        XCTAssertEqual(queried, paths.sorted())
    }

    func testMultipleReceiptsAreDeduplicatedAndSorted() throws {
        let observer = NativeInstallerReceiptObserver { try ReceiptFixtures.reply(path: $0, identifiers: ["z.pkg", "a.pkg", "z.pkg"]) }
        XCTAssertEqual(try observer.snapshot(target: target, paths: paths).identifiers, ["a.pkg", "z.pkg"])
    }

    func testMalformedOrMisdirectedRepliesAreNotEmptyInventory() throws {
        let invalid: [Any] = [[], ["path": target.path], ["path": target.path, "path-info": ""],
                              ["path": "/different.app", "path-info": []],
                              ["path": target.path, "path-info": [], "unexpected": true],
                              ["path": target.path, "path-info": [[:]]],
                              ["path": target.path, "path-info": [["pkgid": 1]]]]
        for value in invalid {
            let data = try PropertyListSerialization.data(fromPropertyList: value, format: .xml, options: 0)
            XCTAssertThrowsError(try NativeInstallerReceiptObserver.identifiers(data, path: target.path))
        }
        for data in [Data(), Data("invalid".utf8), Data(repeating: 0, count: 65_537)] {
            XCTAssertThrowsError(try NativeInstallerReceiptObserver.identifiers(data, path: target.path))
        }
    }

    func testInvalidAndExcessiveClaimsFailClosed() throws {
        for identifiers in [[""], [" leading"], ["line\nbreak"], [String(repeating: "x", count: 256)], Array(repeating: "pkg", count: 129)] {
            let data = try ReceiptFixtures.reply(path: target.path, identifiers: identifiers)
            XCTAssertThrowsError(try NativeInstallerReceiptObserver.identifiers(data, path: target.path))
        }
    }

    func testInvalidScopeNeverQueriesSystem() {
        var calls = 0
        let observer = NativeInstallerReceiptObserver { _ in calls += 1; return Data() }
        for scope in [[], [target.path, target.path], [paths[1]],
                      [target.path, "/Applications/Other.app"], [target.path, target.path + ".other/Child"],
                      [target.path, target.path + "/../Other"], [target.path, target.path + "/./Other"],
                      [target.path, target.path + "//Child"], [target.path, target.path + "/Child/"],
                      [target.path, target.path + "/Bad\0"], [target.path, target.path + "/Line\n"],
                      [target.path, target.path + "/" + String(repeating: "x", count: 4096)]] {
            XCTAssertThrowsError(try observer.snapshot(target: target, paths: scope))
        }
        XCTAssertEqual(calls, 0)
    }

    func testQueryFailureIsNotAnEmptySnapshot() {
        let observer = NativeInstallerReceiptObserver { _ in throw ObservationFailure.unreadableManagerEvidence }
        XCTAssertThrowsError(try observer.snapshot(target: target, paths: paths))
    }

    func testReceiptMetadataDriftChangesSnapshotEvenWithSamePackageID() throws {
        var revision = 1
        let observer = NativeInstallerReceiptObserver { path in
            try PropertyListSerialization.data(fromPropertyList: ["path": path, "path-info": [["pkgid": "same.pkg", "install-time": revision]]],
                                               format: .xml, options: 0)
        }
        let before = try observer.snapshot(target: target, paths: paths)
        revision = 2
        let after = try observer.snapshot(target: target, paths: paths)
        XCTAssertEqual(before.identifiers, after.identifiers)
        XCTAssertNotEqual(before, after)
    }

    func testStructuredArgumentsDoNotInvokeAShell() throws {
        let text = "a; $(false) ' & /tmp/no-shell"
        let data = try BoundedSystemQuery.run(executable: "/usr/bin/printf", arguments: ["%s", text])
        XCTAssertEqual(String(data: data, encoding: .utf8), text)
    }

    func testEnvironmentIsFixedAndNoInputIsRead() throws {
        let data = try BoundedSystemQuery.run(executable: "/usr/bin/env", arguments: [])
        let values = Set(try XCTUnwrap(String(data: data, encoding: .utf8)).split(separator: "\n").map(String.init))
        XCTAssertEqual(values, ["PATH=/usr/bin:/bin:/usr/sbin:/sbin", "HOME=/var/empty", "LANG=C", "LC_ALL=C"])
        XCTAssertTrue(try BoundedSystemQuery.run(executable: "/bin/cat", arguments: []).isEmpty)
        XCTAssertEqual(try BoundedSystemQuery.run(executable: "/bin/pwd", arguments: []), Data("/\n".utf8))
    }

    func testNonzeroExitAndDiagnosticsRejectEvenWithOutput() {
        XCTAssertThrowsError(try BoundedSystemQuery.run(executable: "/usr/bin/false", arguments: []))
        XCTAssertThrowsError(try BoundedSystemQuery.run(executable: "/usr/bin/printf", arguments: ["%Q"]))
        XCTAssertThrowsError(try BoundedSystemQuery.run(executable: "/does-not-exist", arguments: []))
        XCTAssertThrowsError(try BoundedSystemQuery.run(executable: "/usr/bin/printf", arguments: ["hello\0hidden"]))
    }

    func testTimeoutTerminatesAndReapsOwnedChild() throws {
        let start = DispatchTime.now().uptimeNanoseconds
        XCTAssertThrowsError(try BoundedSystemQuery.run(executable: "/bin/sleep", arguments: ["30"], timeoutNanoseconds: 30_000_000))
        XCTAssertLessThan(DispatchTime.now().uptimeNanoseconds - start, 2_000_000_000)
        XCTAssertEqual(try BoundedSystemQuery.run(executable: "/usr/bin/printf", arguments: ["ok"]), Data("ok".utf8))
    }

    func testOutputFloodIsBoundedAndDoesNotDeadlock() throws {
        XCTAssertThrowsError(try BoundedSystemQuery.run(executable: "/usr/bin/yes", arguments: [], maximumBytes: 4096)) {
            XCTAssertEqual($0 as? ObservationFailure, .limitExceeded)
        }
        XCTAssertEqual(try BoundedSystemQuery.run(executable: "/usr/bin/printf", arguments: ["1234"], maximumBytes: 4), Data("1234".utf8))
        XCTAssertThrowsError(try BoundedSystemQuery.run(executable: "/usr/bin/printf", arguments: ["12345"], maximumBytes: 4))
    }

    func testActualReadOnlySystemQueryParsesForUniqueUnclaimedPath() throws {
        // Runtime is restricted to VM/CI. This does not install/forget a receipt.
        let path = "/Applications/Helm-Receipt-Test-\(UUID().uuidString).app"
        let reply = try NativeInstallerReceiptObserver().batchQuery([path], 1_000_000_000)
        XCTAssertTrue(try NativeInstallerReceiptObserver.identifiers(reply, path: path).isEmpty)
    }

    func testActualSystemQueryReturnsEveryRepeatedOption() throws {
        let path = "/Applications/Helm-Receipt-Test-\(UUID().uuidString).app"
        let paths = [path, path + "/Contents/Resources/a & b's.dat", path + "/Contents/Info.plist"]
        let reply = try NativeInstallerReceiptObserver().batchQuery(paths, 1_000_000_000)
        XCTAssertTrue(try NativeInstallerReceiptObserver.batchIdentifiers(reply, paths: paths).isEmpty)
    }
}
