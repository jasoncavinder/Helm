import XCTest

final class HelmCliShimCommandRunnerTests: XCTestCase {
    func testDrainsStandardOutputAndError() throws {
        let result = try run(
            executableURL: URL(fileURLWithPath: "/bin/sh"),
            arguments: ["-c", "yes x | head -c 131072; yes y | head -c 131072 >&2"],
            timeout: 5
        )

        XCTAssertNil(result.launchError)
        XCTAssertFalse(result.didTimeout)
        XCTAssertEqual(result.terminationStatus, 0)
        XCTAssertGreaterThanOrEqual(result.stdout.count, 131_072)
        XCTAssertGreaterThanOrEqual(result.stderr.count, 131_072)
    }

    func testTimeoutTerminatesProcess() throws {
        let start = Date()
        let result = try run(
            executableURL: URL(fileURLWithPath: "/bin/sleep"),
            arguments: ["30"],
            timeout: 0.1
        )

        XCTAssertNil(result.launchError)
        XCTAssertTrue(result.didTimeout)
        XCTAssertLessThan(Date().timeIntervalSince(start), 5)
    }

    private func run(
        executableURL: URL,
        arguments: [String],
        timeout: TimeInterval
    ) throws -> HelmCliShimCommandResult {
        let expectation = expectation(description: "CLI command completes")
        var commandResult: HelmCliShimCommandResult?

        HelmCliShimCommandRunner().run(
            executableURL: executableURL,
            arguments: arguments,
            timeout: timeout
        ) { result in
            commandResult = result
            expectation.fulfill()
        }

        wait(for: [expectation], timeout: 10)
        return try XCTUnwrap(commandResult)
    }
}
