import Foundation
import Combine

class LocalizationManager: ObservableObject {
    static let shared = LocalizationManager()
    
    @Published var currentLocale: String = "en" {
        didSet {
            if oldValue != currentLocale {
                loadLocale(currentLocale)
                UserDefaults.standard.set(currentLocale, forKey: kLocaleKey)
            }
        }
    }

    private let defaultLocale = "en"
    private let localeFiles = ["common", "app", "service"]
    private var strings: [String: String] = [:]
    private var fallbackStrings: [String: String] = [:]
    private let kLocaleKey = "user_locale_preference"
    
    private init() {
        let saved = UserDefaults.standard.string(forKey: kLocaleKey)
        let locale = saved ?? preferredSystemLocale()
        fallbackStrings = loadStrings(for: defaultLocale)
        currentLocale = locale
        loadLocale(locale)
    }

    private func preferredSystemLocale() -> String {
        Locale.preferredLanguages.first ?? Locale.current.identifier
    }
    
    private func loadLocale(_ locale: String) {
        var loadedStrings: [String: String] = [:]
        let chain = localeFallbackChain(for: locale)

        // Load from least-specific to most-specific so locale-specific values override defaults.
        for candidate in chain.reversed() {
            let partial = loadStrings(for: candidate)
            loadedStrings.merge(partial) { (_, new) in new }
        }

        self.strings = loadedStrings.isEmpty ? fallbackStrings : loadedStrings
    }

    private func localeFallbackChain(for locale: String) -> [String] {
        let trimmed = locale.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty {
            return [defaultLocale]
        }

        let normalized = trimmed.replacingOccurrences(of: "_", with: "-")
        let language = normalized.split(separator: "-").first.map(String.init)

        var chain: [String] = [normalized]
        if let language, language != normalized {
            chain.append(language)
        }
        chain.append(defaultLocale)

        var seen = Set<String>()
        return chain.filter { candidate in
            if seen.contains(candidate) {
                return false
            }
            seen.insert(candidate)
            return true
        }
    }

    private func loadStrings(for locale: String) -> [String: String] {
        var loadedStrings: [String: String] = [:]

        for file in localeFiles {
            let urls = candidateFileURLs(for: file, locale: locale)
            var loaded = false
            for url in urls {
                do {
                    let data = try Data(contentsOf: url)
                    if let json = try JSONSerialization.jsonObject(with: data, options: []) as? [String: String] {
                        loadedStrings.merge(json) { (_, new) in new }
                        loaded = true
                        break
                    }
                } catch {
                    print("LocalizationManager: Failed to load \(file).json for locale \(locale): \(error)")
                }
            }

            if !loaded {
                print("LocalizationManager: Could not find \(file).json for locale \(locale) in bundle.")
            }
        }

        return loadedStrings
    }

    private func candidateFileURLs(for file: String, locale: String) -> [URL] {
        var urls: [URL] = []

        if let scoped = Bundle.main.url(
            forResource: file,
            withExtension: "json",
            subdirectory: "locales/\(locale)"
        ) {
            urls.append(scoped)
        }

        // Flat fallback is only safe for default locale.
        if locale == defaultLocale,
           let flat = Bundle.main.url(forResource: file, withExtension: "json")
        {
            urls.append(flat)
        }

        return urls
    }

    private func intValue(for value: Any?) -> Int? {
        switch value {
        case let intValue as Int:
            return intValue
        case let doubleValue as Double:
            return Int(doubleValue)
        case let stringValue as String:
            return Int(stringValue)
        default:
            return nil
        }
    }

    private func applySimpleArguments(_ format: String, args: [String: Any]) -> String {
        var result = format
        for (argKey, argValue) in args {
            let placeholder = "{\(argKey)}"
            result = result.replacingOccurrences(of: placeholder, with: "\(argValue)")
        }
        return result
    }

    private func applyPluralArguments(_ format: String, args: [String: Any]) -> String {
        let pattern = #"\{([a-zA-Z0-9_]+),\s*plural,\s*one\s*\{([^{}]*)\}\s*other\s*\{([^{}]*)\}\s*\}"#
        guard let regex = try? NSRegularExpression(pattern: pattern, options: []) else {
            return format
        }

        let fullRange = NSRange(format.startIndex..<format.endIndex, in: format)
        let matches = regex.matches(in: format, options: [], range: fullRange)
        if matches.isEmpty {
            return format
        }

        var result = format
        for match in matches.reversed() {
            guard
                let fullRange = Range(match.range(at: 0), in: result),
                let keyRange = Range(match.range(at: 1), in: result),
                let oneRange = Range(match.range(at: 2), in: result),
                let otherRange = Range(match.range(at: 3), in: result)
            else {
                continue
            }

            let key = String(result[keyRange])
            let oneTemplate = String(result[oneRange])
            let otherTemplate = String(result[otherRange])
            let count = intValue(for: args[key]) ?? 0
            let selected = (count == 1 ? oneTemplate : otherTemplate)
                .replacingOccurrences(of: "#", with: "\(count)")

            result.replaceSubrange(fullRange, with: selected)
        }

        return result
    }
    
    func string(_ key: String, args: [String: Any] = [:]) -> String {
        let format = strings[key] ?? fallbackStrings[key]
        guard let format else {
            #if DEBUG
            return "⟦\(key)⟧"
            #else
            print("LocalizationManager: Missing key \(key) for locale \(currentLocale)")
            return key
            #endif
        }

        let pluralApplied = applyPluralArguments(format, args: args)
        return applySimpleArguments(pluralApplied, args: args)
    }
}
