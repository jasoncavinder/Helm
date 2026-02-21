import XCTest

final class SupportRedactorTests: XCTestCase {
    func testRedactsHomeDirectoryEmailAndGitHubTokens() {
        let home = NSHomeDirectory()
        let raw = """
        userPath=\(home)/workspace
        explicitPath=/Users/example-user/bin
        email=person@example.com
        token=ghp_1234567890abcdefghijABCDEFGHIJ
        fineGrained=github_pat_1234567890abcdefghijklmnop_qwerty
        """

        var redactor = SupportRedactor()
        let redacted = redactor.redactString(raw)

        XCTAssertFalse(redacted.contains(home))
        XCTAssertTrue(redacted.contains("~/workspace"))

        XCTAssertFalse(redacted.contains("/Users/example-user"))
        XCTAssertTrue(redacted.contains("/Users/[redacted-user]/bin"))

        XCTAssertFalse(redacted.contains("person@example.com"))
        XCTAssertTrue(redacted.contains("[redacted-email]"))

        XCTAssertFalse(redacted.contains("ghp_1234567890abcdefghijABCDEFGHIJ"))
        XCTAssertFalse(redacted.contains("github_pat_1234567890abcdefghijklmnop_qwerty"))
        XCTAssertTrue(redacted.contains("[redacted-token]"))

        XCTAssertTrue(redactor.appliedRules.contains("home_directory"))
        XCTAssertTrue(redactor.appliedRules.contains("user_path"))
        XCTAssertTrue(redactor.appliedRules.contains("email"))
        XCTAssertTrue(redactor.appliedRules.contains("github_token"))
        XCTAssertGreaterThan(redactor.replacementCount, 0)
    }

    func testOptionalAndDictionaryHelpersApplySameRedaction() {
        var redactor = SupportRedactor()
        let email = "test@example.com"
        let token = "ghs_1234567890abcdefghijABCDEFGHIJ"
        let path = "/Users/someone"
        let input: [String: String] = [
            "email": email,
            "token": token,
            "path": path,
        ]

        let optionalRedacted = redactor.redactOptionalString(email)
        let dictRedacted = redactor.redactDictionary(input)

        XCTAssertEqual(optionalRedacted, "[redacted-email]")
        XCTAssertEqual(dictRedacted?["email"], "[redacted-email]")
        XCTAssertEqual(dictRedacted?["token"], "[redacted-token]")
        XCTAssertEqual(dictRedacted?["path"], "/Users/[redacted-user]")
    }
}
