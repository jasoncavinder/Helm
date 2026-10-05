import Darwin
import Foundation
import XCTest
@testable import HelmExternalUpdateObservation

final class MacPortsObservationTests: XCTestCase {
    private var root: URL!
    private var registry: URL!
    private let target = URL(fileURLWithPath: "/Applications/Example.app", isDirectory: true)
    private let version = #"{"kind":"version","path":"1.215","actual_path":null,"active":null}"#

    override func setUpWithError() throws {
        root = FileManager.default.homeDirectoryForCurrentUser.resolvingSymlinksInPath()
            .appendingPathComponent("helm-macports-observation-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        registry = root.appendingPathComponent("registry.db", isDirectory: false)
    }

    override func tearDownWithError() throws { try FileManager.default.removeItem(at: root) }

    private func reply(_ rows: String = "") -> Data { Data("[\(version)\(rows.isEmpty ? "" : "," + rows)]".utf8) }

    private func fakeObserver() throws -> NativeMacPortsObserver {
        try Data("not parsed by the injected query".utf8).write(to: registry)
        var observer = NativeMacPortsObserver(registry: registry)
        observer.read = { _ in self.reply() }
        return observer
    }

    func testMissingRegistryDoesNotRunQueryOrCreateFiles() throws {
        var observer = NativeMacPortsObserver(registry: registry)
        observer.read = { _ in XCTFail("missing registry queried"); return Data() }
        XCTAssertTrue(try observer.snapshot(target: target).claims.isEmpty)
        XCTAssertFalse(FileManager.default.fileExists(atPath: registry.path))
    }

    func testFixedReadOnlyArgumentsIgnoreSqliteStartupConfiguration() throws {
        var observer = try fakeObserver()
        observer.read = { args in
            XCTAssertEqual(Array(args.prefix(6)), ["-batch", "-safe", "-readonly", "-init", "/dev/null", "-json"])
            let uri = try XCTUnwrap(URLComponents(string: args[6]))
            XCTAssertEqual(uri.path, self.registry.path)
            XCTAssertEqual(uri.queryItems, [URLQueryItem(name: "mode", value: "ro"), URLQueryItem(name: "immutable", value: "1")])
            XCTAssertEqual(args.count, 8)
            XCTAssertEqual(args[7], NativeMacPortsObserver.query)
            return self.reply()
        }
        XCTAssertTrue(try observer.snapshot(target: target).claims.isEmpty)
    }

    func testExactPayloadCaseAndActualPathClaimsAreDeniedWithoutPrefixCollisions() throws {
        let rows = #"{"kind":"file","path":"/Applications/EXAMPLE.app/Contents/Code","actual_path":"/Applications/Example.app/Contents/Code","active":1},{"kind":"file","path":"/elsewhere","actual_path":"/Applications/Example.app","active":1},{"kind":"file","path":"/Applications/Example.app2/no","active":0},{"kind":"file","path":"/Applications","active":0}"#
        XCTAssertEqual(try NativeMacPortsObserver.claims(reply(rows), target: target.path), [
            "/Applications/EXAMPLE.app/Contents/Code", "/Applications/Example.app", "/Applications/Example.app/Contents/Code"
        ])
    }

    func testInactiveRecordsRemainDenialsAndUnicodeSpellingIsConservative() throws {
        let rows = #"{"kind":"file","path":"/Applications/Café.app/file","actual_path":null,"active":0}"#
        XCTAssertEqual(try NativeMacPortsObserver.claims(reply(rows), target: "/Applications/Cafe\u{301}.app"),
                       ["/Applications/Café.app/file"])
    }

    func testUnsupportedSchemaMalformedRowsAndRelativePathsFailClosed() throws {
        let bad = [Data("[\(version.replacingOccurrences(of: "1.215", with: "1.214"))]".utf8), Data("[]".utf8), Data("invalid".utf8), reply(version),
            reply(#"{"kind":"file","path":"/x","active":1}"#),
            reply(#"{"kind":"file","path":"/x","active":true}"#),
            reply(#"{"kind":"file","path":"/x","active":2}"#),
            reply(#"{"kind":"file","path":"relative","active":0}"#),
            reply(#"{"kind":"file","path":"/x/../y","active":0}"#),
            reply(#"{"kind":"file","path":"/x","actual_path":"/x\u0000y","active":1}"#)]
        for data in bad { XCTAssertThrowsError(try NativeMacPortsObserver.claims(data, target: target.path)) }
        XCTAssertThrowsError(try NativeMacPortsObserver.claims(Data(repeating: 0, count: NativeMacPortsObserver.maximumBytes + 1), target: target.path))
    }

    func testOversizedRegistryFailsBeforeQuery() throws {
        let observer = try fakeObserver()
        let file = try FileHandle(forWritingTo: registry)
        try file.truncate(atOffset: 64 * 1024 * 1024 + 1)
        try file.close()
        XCTAssertThrowsError(try observer.snapshot(target: target))
    }

    func testSymlinksNonregularFilesAndAliasedParentsFailBeforeQuery() throws {
        var observer = try fakeObserver()
        observer.read = { _ in XCTFail("unsafe path queried"); return self.reply() }
        let original = root.appendingPathComponent("original.db", isDirectory: false)
        try FileManager.default.moveItem(at: registry, to: original)
        try FileManager.default.createSymbolicLink(at: registry, withDestinationURL: original)
        XCTAssertThrowsError(try observer.snapshot(target: target))
        try FileManager.default.removeItem(at: registry)
        XCTAssertEqual(mkfifo(registry.path, 0o600), 0)
        XCTAssertThrowsError(try observer.snapshot(target: target))
        try FileManager.default.removeItem(at: registry)
        let alias = root.appendingPathComponent("alias", isDirectory: true)
        try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: root)
        XCTAssertThrowsError(try NativeMacPortsObserver(registry: alias.appendingPathComponent("missing.db", isDirectory: false)).snapshot(target: target))
    }

    func testNonemptyJournalsAndOrphanedSidecarsAreNeverIgnored() throws {
        let observer = try fakeObserver()
        for suffix in ["-wal", "-journal"] {
            let sidecar = URL(fileURLWithPath: registry.path + suffix, isDirectory: false)
            try Data([1]).write(to: sidecar)
            XCTAssertThrowsError(try observer.snapshot(target: target))
            try FileManager.default.removeItem(at: sidecar)
        }
        try FileManager.default.removeItem(at: registry)
        try Data().write(to: URL(fileURLWithPath: registry.path + "-wal", isDirectory: false))
        XCTAssertThrowsError(try observer.snapshot(target: target))
    }

    func testEmptyWalAndExistingShmArePreservedAndBoundIntoSnapshot() throws {
        let observer = try fakeObserver()
        try Data().write(to: URL(fileURLWithPath: registry.path + "-wal", isDirectory: false))
        try Data(repeating: 0, count: 32768).write(to: URL(fileURLWithPath: registry.path + "-shm", isDirectory: false))
        let snapshot = try observer.snapshot(target: target)
        XCTAssertNotNil(snapshot.filesystem[registry.path + "-wal"])
        XCTAssertNotNil(snapshot.filesystem[registry.path + "-shm"])
        XCTAssertEqual(try observer.snapshot(target: target), snapshot)
    }

    func testRegistryOrSidecarChangesDuringQueryRejectResult() throws {
        for sidecar in [false, true] {
            var observer = try fakeObserver()
            observer.read = { _ in
                if sidecar {
                    try Data().write(to: URL(fileURLWithPath: self.registry.path + "-wal", isDirectory: false))
                } else { try Data("changed registry".utf8).write(to: self.registry) }
                return self.reply()
            }
            XCTAssertThrowsError(try observer.snapshot(target: target))
        }
    }

    func testQueryFailureIsNotUnclaimed() throws {
        var observer = try fakeObserver()
        observer.read = { _ in throw ObservationFailure.limitExceeded }
        XCTAssertThrowsError(try observer.snapshot(target: target))
    }

    func testRealSystemSqliteReadsFixtureWithoutChangingIt() throws {
        _ = try BoundedSystemQuery.run(executable: "/usr/bin/sqlite3", arguments: ["-init", "/dev/null", registry.path, """
            CREATE TABLE metadata (key, value);
            INSERT INTO metadata VALUES ('version', '1.215');
            CREATE TABLE files (id INTEGER, path TEXT, actual_path TEXT, active INTEGER);
            INSERT INTO files VALUES (1, '/Applications/Example.app/payload', '/Applications/Example.app/payload', 1);
            """], timeoutNanoseconds: 1_000_000_000, maximumBytes: 4096)
        let before = try Data(contentsOf: registry)
        let children = try FileManager.default.contentsOfDirectory(atPath: root.path).sorted()
        let snapshot = try NativeMacPortsObserver(registry: registry).snapshot(target: target)
        XCTAssertEqual(snapshot.claims, ["/Applications/Example.app/payload"])
        XCTAssertEqual(try Data(contentsOf: registry), before)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: root.path).sorted(), children)
    }

    func testLocationExclusionIsComponentBoundAndNeverAnAllow() {
        for (path, expected) in [("/Applications/MacPorts/Example.app", true), ("/applications/MACPORTS/Example.app", true),
                                 ("/Applications/MacPortsOther/Example.app", false), ("/Applications/Example.app", false)] {
            let evidence = NativeManagerObserver.Snapshot().evidence(target: URL(fileURLWithPath: path, isDirectory: true),
                                                                     applicationRoots: [], hasStoreReceipt: false)
            XCTAssertEqual(evidence.exclusions.contains(.macportsLocation), expected)
            XCTAssertEqual(evidence.disposition, expected ? .otherManager : .unresolved)
        }
    }

    func testRegistryViewsCannotInvokeUnsafeFileFunctions() throws {
        _ = try BoundedSystemQuery.run(executable: "/usr/bin/sqlite3", arguments: ["-init", "/dev/null", registry.path, """
            CREATE TABLE metadata (key, value);
            INSERT INTO metadata VALUES ('version', '1.215');
            CREATE VIEW files AS SELECT readfile('/etc/hosts') AS path, '/x' AS actual_path, 1 AS active;
            """], timeoutNanoseconds: 1_000_000_000, maximumBytes: 4096)
        XCTAssertThrowsError(try NativeMacPortsObserver(registry: registry).snapshot(target: target))
    }
}
