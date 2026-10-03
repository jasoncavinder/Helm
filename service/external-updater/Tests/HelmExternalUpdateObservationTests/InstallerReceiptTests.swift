import Darwin
import Foundation
import XCTest
@testable import HelmExternalUpdateObservation

final class InstallerReceiptTests: XCTestCase {
    private let target = URL(fileURLWithPath: "/Applications/Space & Quote's.app", isDirectory: true)

    func testEmptyResponsesRemainOnlyAbsenceOfTheseMarkers() throws {
        let snapshot = try ReceiptFixtures.empty.snapshot(target: target, executable: "Main")
        XCTAssertEqual(snapshot.replies.count, 3)
        XCTAssertTrue(snapshot.identifiers.isEmpty)
    }

    func testExactlyBundleInfoAndExecutableAreQueried() throws {
        var paths: [String] = []
        let observer = NativeInstallerReceiptObserver { path in
            paths.append(path)
            return try ReceiptFixtures.reply(path: path, identifiers: path.hasSuffix("/Main") ? ["org.example.installer"] : [])
        }
        XCTAssertEqual(try observer.snapshot(target: target, executable: "Main").identifiers, ["org.example.installer"])
        XCTAssertEqual(paths, [target.path, target.path + "/Contents/Info.plist", target.path + "/Contents/MacOS/Main"])
    }

    func testMultipleReceiptsAreDeduplicatedAndSorted() throws {
        let observer = NativeInstallerReceiptObserver { try ReceiptFixtures.reply(path: $0, identifiers: ["z.pkg", "a.pkg", "z.pkg"]) }
        XCTAssertEqual(try observer.snapshot(target: target, executable: "Main").identifiers, ["a.pkg", "z.pkg"])
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

    func testUnsafeExecutableNameNeverQueriesSystem() {
        var calls = 0
        let observer = NativeInstallerReceiptObserver { _ in calls += 1; return Data() }
        for name in ["", ".", "..", "../Other", "/bin/sh", "Main\0", String(repeating: "x", count: 256)] {
            XCTAssertThrowsError(try observer.snapshot(target: target, executable: name))
        }
        XCTAssertEqual(calls, 0)
    }

    func testQueryFailureIsNotAnEmptySnapshot() {
        let observer = NativeInstallerReceiptObserver { _ in throw ObservationFailure.unreadableManagerEvidence }
        XCTAssertThrowsError(try observer.snapshot(target: target, executable: "Main"))
    }

    func testReceiptMetadataDriftChangesSnapshotEvenWithSamePackageID() throws {
        var revision = 1
        let observer = NativeInstallerReceiptObserver { path in
            try PropertyListSerialization.data(fromPropertyList: ["path": path, "path-info": [["pkgid": "same.pkg", "install-time": revision]]],
                                               format: .xml, options: 0)
        }
        let before = try observer.snapshot(target: target, executable: "Main")
        revision = 2
        let after = try observer.snapshot(target: target, executable: "Main")
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
        let reply = try NativeInstallerReceiptObserver().query(path)
        XCTAssertTrue(try NativeInstallerReceiptObserver.identifiers(reply, path: path).isEmpty)
    }
}
