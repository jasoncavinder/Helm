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

final class InspectorDescriptionRenderingTests: XCTestCase {
    func testLooksLikeHTMLDetectsTagsAndEntities() {
        XCTAssertTrue(PackageDescriptionRenderer.looksLikeHTML("<b>bold</b> text"))
        XCTAssertTrue(PackageDescriptionRenderer.looksLikeHTML("Tom &amp; Jerry"))
        XCTAssertFalse(PackageDescriptionRenderer.looksLikeHTML("plain summary text"))
    }

    func testRenderReturnsRichDescriptionForHTML() {
        let rendered = PackageDescriptionRenderer.render("<p>Hello <strong>World</strong></p>")
        switch rendered {
        case .rich(let attributed):
            XCTAssertTrue(attributed.string.contains("Hello"))
            XCTAssertTrue(attributed.string.contains("World"))
        default:
            XCTFail("Expected rich HTML rendering result")
        }
    }

    func testRenderReturnsPlainDescriptionForNonHTML() {
        let rendered = PackageDescriptionRenderer.render("Simple plain summary")
        switch rendered {
        case .plain(let text):
            XCTAssertEqual(text, "Simple plain summary")
        default:
            XCTFail("Expected plain rendering result")
        }
    }
}

final class InspectorLinkPolicyTests: XCTestCase {
    func testSafeURLAllowsOnlyHttpAndHttps() {
        XCTAssertEqual(
            InspectorLinkPolicy.safeURL(from: "https://helmapp.dev/docs")?.absoluteString,
            "https://helmapp.dev/docs"
        )
        XCTAssertEqual(
            InspectorLinkPolicy.safeURL(from: URL(string: "http://example.com")!)?.absoluteString,
            "http://example.com"
        )
        XCTAssertNil(InspectorLinkPolicy.safeURL(from: "file:///tmp/readme.txt"))
        XCTAssertNil(InspectorLinkPolicy.safeURL(from: "javascript:alert(1)"))
        XCTAssertNil(InspectorLinkPolicy.safeURL(from: "mailto:test@example.com"))
    }
}
