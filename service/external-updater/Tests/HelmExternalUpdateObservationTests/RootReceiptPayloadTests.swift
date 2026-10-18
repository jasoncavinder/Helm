import Foundation
import XCTest
@testable import HelmExternalUpdateObservation

final class RootReceiptPayloadTests: XCTestCase {
    private let target = URL(fileURLWithPath: "/Applications/Example.app", isDirectory: true)

    private func payload(_ text: String, identifier: String = "org.example.pkg", target: URL? = nil) throws -> NativeRootReceiptPayload {
        try NativeRootReceiptPayload(identifier: identifier, data: Data(text.utf8), target: target ?? self.target,
                                     remaining: { 3_000_000_000 })
    }

    func testRootAndHistoricalPathsClaimWithoutFilesystemProbes() throws {
        for path in ["Applications/Example.app", "./Applications/Example.app/Contents/Deleted", "applications/EXAMPLE.APP/Old"] {
            XCTAssertTrue(try payload(path + "\n").claimsTarget)
        }
        for path in [".", "Applications", "Applications/Example.app.other/Old", "Applications/Other.app", "Library/Example.app"] {
            XCTAssertFalse(try payload(path + "\n").claimsTarget)
        }
    }

    func testAccountApplicationsAndUnicodeCaseEquivalentPaths() throws {
        let unicode = URL(fileURLWithPath: "/Users/agent/Applications/Caf\u{E9}.app", isDirectory: true)
        XCTAssertTrue(try payload("users/AGENT/applications/CAFE\u{301}.APP/Old\n", target: unicode).claimsTarget)
        XCTAssertFalse(try payload("Users/another/Applications/Caf\u{E9}.app\n", target: unicode).claimsTarget)
        let unicodeRoot = URL(fileURLWithPath: "/Caf\u{E9}/Example.app", isDirectory: true)
        XCTAssertTrue(try payload("CAFE\u{301}/Example.app/Old\n", target: unicodeRoot).claimsTarget)
    }

    func testEveryRawByteAndReceiptIdentityParticipateInBinding() throws {
        let first = try payload("Applications/Other.app\nLibrary/Other\n")
        let reordered = try payload("Library/Other\nApplications/Other.app\n")
        let caseChanged = try payload("APPLICATIONS/Other.app\nLibrary/Other\n")
        let identifierChanged = try payload("Applications/Other.app\nLibrary/Other\n", identifier: "org.another.pkg")
        for changed in [reordered, caseChanged, identifierChanged] {
            XCTAssertEqual(first.claimsTarget, changed.claimsTarget)
            XCTAssertNotEqual(first, changed)
        }
        XCTAssertEqual(first, try payload("Applications/Other.app\nLibrary/Other\n"))
    }

    func testEmptySuccessfulPayloadRemainsAnEmptyBoundObservation() throws {
        let empty = try payload("")
        XCTAssertEqual(empty.pathCount, 0)
        XCTAssertEqual(empty.byteCount, 0)
        XCTAssertEqual(empty.digest.count, 32)
        XCTAssertFalse(empty.claimsTarget)
        XCTAssertNotEqual(empty, try payload(".\n"))
    }

    func testMalformedTruncatedAndUnsafePathsRejectEvenAfterAClaim() throws {
        for value in ["Applications/Example.app", "\n", "a\n\n", "/Applications/Example.app\n", "../Applications/Example.app\n",
                      "Applications/../Example.app\n", "Library/../Applications/Example.app\n", "Library/a\u{0}b\n", "Library/\u{85}\n",
                      "a\u{0}b\n", "a\tb\n", "a\rb\n", String(repeating: "a", count: 4097) + "\n"] {
            XCTAssertThrowsError(try payload(value))
            XCTAssertThrowsError(try payload("Applications/Example.app\n" + value))
        }
    }

    func testLineFramingCanOnlyBroadenDenialsForEmbeddedNewlines() throws {
        // pkgutil's text listing cannot disambiguate embedded newlines. With a
        // newline-free target, a real child still retains the target prefix;
        // a false extra path can only add a conservative denial, never hide it.
        XCTAssertTrue(try payload("Applications/Example.app/line\nbreak\n").claimsTarget)
        XCTAssertTrue(try payload("Elsewhere\nApplications/Example.app/Old\n").claimsTarget)
    }

    func testBoundsRejectWithoutReturningPartialEvidence() throws {
        XCTAssertThrowsError(try payload(String(repeating: "a", count: NativeRootReceiptPayload.maximumBytes + 1)))
        XCTAssertThrowsError(try payload(String(repeating: "a\n", count: NativeRootReceiptPayload.maximumPaths + 1)))
        XCTAssertThrowsError(try NativeRootReceiptPayload(identifier: "org.example.pkg", data: Data([0xFF, 10]), target: target,
                                                         remaining: { 1 }))
    }

    func testDeadlineCheckedDuringParsingAndAfterHashing() throws {
        for limit in [0, 1, 2, 3] {
            var calls = 0
            XCTAssertThrowsError(try NativeRootReceiptPayload(identifier: "org.example.pkg",
                data: Data(String(repeating: "Library/Other\n", count: 257).utf8), target: target, remaining: {
                    defer { calls += 1 }
                    guard calls < limit else { throw ObservationFailure.limitExceeded }
                    return 1
                }))
        }
    }

}
