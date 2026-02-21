import Foundation

struct SupportRedactor {
    private(set) var appliedRules: Set<String> = []
    private(set) var replacementCount: Int = 0
    private let homeDirectory = NSHomeDirectory()

    mutating func redactString(_ raw: String) -> String {
        var value = raw
        value = applyLiteral(
            rule: "home_directory",
            value: value,
            target: homeDirectory,
            replacement: "~"
        )
        value = applyRegex(
            rule: "user_path",
            value: value,
            pattern: #"/Users/[^/\s]+"#,
            replacement: "/Users/[redacted-user]"
        )
        value = applyRegex(
            rule: "email",
            value: value,
            pattern: #"(?i)\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b"#,
            replacement: "[redacted-email]"
        )
        value = applyRegex(
            rule: "github_token",
            value: value,
            pattern: #"\b(gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,})\b"#,
            replacement: "[redacted-token]"
        )
        return value
    }

    mutating func redactOptionalString(_ raw: String?) -> String? {
        guard let raw else { return nil }
        return redactString(raw)
    }

    mutating func redactDictionary(_ raw: [String: String]?) -> [String: String]? {
        guard let raw else { return nil }
        var redacted: [String: String] = [:]
        for (key, value) in raw {
            redacted[key] = redactString(value)
        }
        return redacted
    }

    private mutating func applyLiteral(
        rule: String,
        value: String,
        target: String,
        replacement: String
    ) -> String {
        guard !target.isEmpty else { return value }
        let count = value.components(separatedBy: target).count - 1
        guard count > 0 else { return value }
        appliedRules.insert(rule)
        replacementCount += count
        return value.replacingOccurrences(of: target, with: replacement)
    }

    private mutating func applyRegex(
        rule: String,
        value: String,
        pattern: String,
        replacement: String
    ) -> String {
        guard let regex = try? NSRegularExpression(pattern: pattern) else {
            return value
        }
        let range = NSRange(value.startIndex..<value.endIndex, in: value)
        let matches = regex.numberOfMatches(in: value, range: range)
        guard matches > 0 else { return value }
        appliedRules.insert(rule)
        replacementCount += matches
        return regex.stringByReplacingMatches(in: value, range: range, withTemplate: replacement)
    }
}
