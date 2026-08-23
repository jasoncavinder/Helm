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

    func testResearchFixtureBlockPreservesManagedShimAndMarker() throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(
            at: directory,
            withIntermediateDirectories: true
        )
        defer { try? FileManager.default.removeItem(at: directory) }

        let shimURL = directory.appendingPathComponent("helm")
        let markerURL = directory.appendingPathComponent("install.json")
        try "#!/bin/sh\n# helm-cli-shim: app-bundle\n".write(
            to: shimURL,
            atomically: true,
            encoding: .utf8
        )
        try "{\"channel\": \"app-bundle-shim\"}\n".write(
            to: markerURL,
            atomically: true,
            encoding: .utf8
        )

        let result: Bool? = try ResearchFixtureLiveOperationGate.perform(
            liveOperationsBlocked: true
        ) {
            try FileManager.default.removeItem(at: shimURL)
            try FileManager.default.removeItem(at: markerURL)
            return true
        }
        XCTAssertNil(result)
        XCTAssertTrue(FileManager.default.fileExists(atPath: shimURL.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: markerURL.path))
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
