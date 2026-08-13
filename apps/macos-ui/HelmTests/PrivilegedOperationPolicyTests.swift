import XCTest

final class PrivilegedOperationPolicyTests: XCTestCase {
    func testAllowsFixedSoftwareUpdateAllShape() throws {
        let command = try PrivilegedOperationPolicy.validate(
            PrivilegedHelperRequest(
                operation: "software_update.install_all",
                program: "/usr/sbin/softwareupdate",
                arguments: ["-i", "-a"]
            )
        )

        XCTAssertEqual(command.executableURL.path, "/usr/sbin/softwareupdate")
        XCTAssertEqual(command.arguments, ["-i", "-a"])
    }

    func testAllowsFixedRosettaInstallShape() throws {
        let command = try PrivilegedOperationPolicy.validate(
            PrivilegedHelperRequest(
                operation: "rosetta.install",
                program: "/usr/sbin/softwareupdate",
                arguments: ["--install-rosetta", "--agree-to-license"]
            )
        )

        XCTAssertEqual(command.arguments, ["--install-rosetta", "--agree-to-license"])
    }

    func testAllowsBoundedXcodeCommandLineToolsLabel() throws {
        let command = try PrivilegedOperationPolicy.validate(
            PrivilegedHelperRequest(
                operation: "xcode_command_line_tools.update",
                program: "/usr/sbin/softwareupdate",
                arguments: ["-i", "Command Line Tools for Xcode-16.3"]
            )
        )

        XCTAssertEqual(command.arguments.last, "Command Line Tools for Xcode-16.3")
    }

    func testRejectsMacAppStoreAndMacPortsOperations() {
        for operation in ["mac_app_store.upgrade", "macports.install"] {
            XCTAssertThrowsError(
                try PrivilegedOperationPolicy.validate(
                    PrivilegedHelperRequest(
                        operation: operation,
                        program: "/tmp/untrusted",
                        arguments: []
                    )
                )
            ) { error in
                XCTAssertEqual(
                    error as? PrivilegedOperationPolicyError,
                    .unsupportedOperation(operation)
                )
            }
        }
    }

    func testRejectsProgramSubstitutionAndArgumentInjection() {
        XCTAssertThrowsError(
            try PrivilegedOperationPolicy.validate(
                PrivilegedHelperRequest(
                    operation: "software_update.install_all",
                    program: "/tmp/softwareupdate",
                    arguments: ["-i", "-a"]
                )
            )
        )
        XCTAssertThrowsError(
            try PrivilegedOperationPolicy.validate(
                PrivilegedHelperRequest(
                    operation: "xcode_command_line_tools.update",
                    program: "/usr/sbin/softwareupdate",
                    arguments: ["-i", "Command Line Tools for Xcode-16.3\n--all"]
                )
            )
        )
    }
}
