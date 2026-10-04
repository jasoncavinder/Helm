import Foundation
import XCTest
@testable import HelmExternalUpdateObservation

final class ReceiptBatchTests: XCTestCase {
    private let target = URL(fileURLWithPath: "/Applications/Batch.app", isDirectory: true)

    private func paths(_ count: Int) -> [String] {
        [target.path] + (1..<count).map { target.path + "/Contents/Resource-\($0)" }
    }

    private func reply(_ paths: [String]) throws -> Data {
        try paths.reduce(into: Data()) { $0.append(try ReceiptFixtures.reply(path: $1)) }
    }

    func testBatchBoundaryKeepsEveryPathAndRemainingBudget() throws {
        var batches: [[String]] = []
        var budgets: [UInt64] = []
        var now: UInt64 = 0
        let observer = NativeInstallerReceiptObserver(batchQuery: { paths, budget in
            batches.append(paths)
            budgets.append(budget)
            now += 500_000_000
            return try self.reply(paths)
        }, clock: { now })
        let snapshot = try observer.snapshot(target: target, paths: paths(65).reversed())
        XCTAssertEqual(batches.map(\.count), [32, 32, 1])
        XCTAssertEqual(batches.flatMap { $0 }, paths(65).sorted())
        XCTAssertEqual(budgets, [3_000_000_000, 2_500_000_000, 2_000_000_000])
        XCTAssertEqual(snapshot.replies.count, 4)
        XCTAssertTrue(snapshot.identifiers.isEmpty)
    }

    func testScopeLimitIsNotTruncated() {
        var calls = 0
        let observer = NativeInstallerReceiptObserver(batchQuery: { _, _ in calls += 1; return Data() })
        XCTAssertThrowsError(try observer.snapshot(target: target, paths: paths(4097))) {
            XCTAssertEqual($0 as? ObservationFailure, .limitExceeded)
        }
        XCTAssertEqual(calls, 0)
    }

    func testMissingDuplicateReorderedExtraOrCorruptDocumentsReject() throws {
        let paths = paths(2)
        let valid = try reply(paths)
        let bad = try [reply([paths[0]]), reply([paths[0], paths[0]]), reply(paths.reversed()),
                       reply(paths + [paths[0]]), valid + Data("garbage".utf8),
                       Data("garbage".utf8) + valid, valid.dropLast(20), Data()]
        for data in bad {
            XCTAssertThrowsError(try NativeInstallerReceiptObserver.batchIdentifiers(Data(data), paths: paths))
        }
        XCTAssertTrue(try NativeInstallerReceiptObserver.batchIdentifiers(valid + Data(" \n\t\r".utf8), paths: paths).isEmpty)
    }

    func testEscapedXMLTextCannotSplitAReply() throws {
        let path = target.path + "/Contents/</plist> & \"quoted\".dat"
        let data = try ReceiptFixtures.reply(path: path, identifiers: ["pkg.</plist>.id"])
        XCTAssertEqual(try NativeInstallerReceiptObserver.batchIdentifiers(data, paths: [path]), ["pkg.</plist>.id"])
    }

    func testQueryBudgetIncludesFinalReplyAndParsing() {
        var now: UInt64 = 10
        let observer = NativeInstallerReceiptObserver(batchQuery: { paths, _ in
            now += NativeInstallerReceiptObserver.snapshotNanoseconds
            return try self.reply(paths)
        }, clock: { now })
        XCTAssertThrowsError(try observer.snapshot(target: target, paths: paths(2)))
    }

    func testBudgetIsSharedAcrossBatches() {
        var now: UInt64 = 0
        var calls = 0
        let observer = NativeInstallerReceiptObserver(batchQuery: { paths, _ in
            calls += 1
            now += 1_100_000_000
            return try self.reply(paths)
        }, clock: { now })
        XCTAssertThrowsError(try observer.snapshot(target: target, paths: paths(100)))
        XCTAssertEqual(calls, 3)
    }

    func testClockRegressionRejectsSnapshot() {
        var now: UInt64 = 10
        let observer = NativeInstallerReceiptObserver(batchQuery: { paths, _ in
            now = 9
            return try self.reply(paths)
        }, clock: { now })
        XCTAssertThrowsError(try observer.snapshot(target: target, paths: paths(2)))
    }

    func testAggregateOutputLimitIsNotPerBatchOnly() {
        var calls = 0
        let observer = NativeInstallerReceiptObserver(batchQuery: { paths, _ in
            calls += 1
            var data = try self.reply(paths)
            data.append(Data(repeating: 32, count: BoundedSystemQuery.maximumBytes - data.count))
            return data
        }, clock: { 0 })
        XCTAssertThrowsError(try observer.snapshot(target: target, paths: paths(4096))) {
            XCTAssertEqual($0 as? ObservationFailure, .limitExceeded)
        }
        XCTAssertEqual(calls, 65)
    }

    func testPerBatchOutputLimitStillApplies() {
        let observer = NativeInstallerReceiptObserver(batchQuery: { _, _ in Data(repeating: 32, count: 65_537) })
        XCTAssertThrowsError(try observer.snapshot(target: target, paths: paths(2)))
    }

    func testBoundedWorkersPreserveSnapshotOrderAndJoinBeforeReturning() throws {
        let state = WorkerState()
        let observer = NativeInstallerReceiptObserver(batchQuery: { paths, _ in
            state.enter()
            defer { state.leave() }
            Thread.sleep(forTimeInterval: paths.contains(self.target.path) ? 0.03 : 0.001)
            return try self.reply(paths)
        }, clock: { 0 }, catalog: ReceiptFixtures.emptyCatalog, workers: 99)
        let result = try observer.snapshot(target: target, paths: paths(160))
        let serial = try ReceiptFixtures.empty.snapshot(target: target, paths: paths(160))
        XCTAssertEqual(result, serial)
        XCTAssertLessThanOrEqual(state.peak, 4)
        XCTAssertEqual(state.active, 0)
        XCTAssertEqual(state.total, 5)
    }

    func testWorkerFailureNeverReturnsPartialSnapshotAndAllChildrenJoin() {
        let state = WorkerState()
        let observer = NativeInstallerReceiptObserver(batchQuery: { paths, _ in
            state.enter()
            defer { state.leave() }
            if paths.contains(self.target.path) { throw ObservationFailure.unreadableManagerEvidence }
            Thread.sleep(forTimeInterval: 0.01)
            return try self.reply(paths)
        }, clock: { 0 }, catalog: ReceiptFixtures.emptyCatalog)
        XCTAssertThrowsError(try observer.snapshot(target: target, paths: paths(160))) {
            XCTAssertEqual($0 as? ObservationFailure, .unreadableManagerEvidence)
        }
        XCTAssertLessThanOrEqual(state.peak, 4)
        XCTAssertEqual(state.active, 0)
    }
}

private final class WorkerState {
    private let lock = NSLock()
    private(set) var active = 0
    private(set) var peak = 0
    private(set) var total = 0

    func enter() {
        lock.lock()
        defer { lock.unlock() }
        active += 1
        total += 1
        peak = max(peak, active)
    }

    func leave() {
        lock.lock()
        defer { lock.unlock() }
        active -= 1
    }
}
