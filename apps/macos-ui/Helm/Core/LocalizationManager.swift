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
    private var strings: [String: String] = [:]
    private let kLocaleKey = "user_locale_preference"
    
    private init() {
        let saved = UserDefaults.standard.string(forKey: kLocaleKey)
        let locale = saved ?? "en"
        currentLocale = locale
        loadLocale(locale)
    }
    
    private func loadLocale(_ locale: String) {
        var loadedStrings: [String: String] = [:]
        let files = ["common", "app", "service"]
        
        // 1. Try to find the locales folder (Folder Reference structure)
        // Expected path: Bundle/Contents/Resources/locales/{locale}/{file}.json
        
        for file in files {
            // Priority 1: Check for explicit subdirectory structure (Folder Reference)
            var fileUrl = Bundle.main.url(forResource: file, withExtension: "json", subdirectory: "locales/\(locale)")
            
            // Priority 2: Check flat structure (Xcode Group flattening) - only works if filenames are unique or we only have one locale
            if fileUrl == nil {
                fileUrl = Bundle.main.url(forResource: file, withExtension: "json")
            }
            
            if let url = fileUrl {
                do {
                    let data = try Data(contentsOf: url)
                    if let json = try JSONSerialization.jsonObject(with: data, options: []) as? [String: String] {
                        loadedStrings.merge(json) { (_, new) in new }
                    }
                } catch {
                    print("LocalizationManager: Failed to load \(file).json for locale \(locale): \(error)")
                }
            } else {
                print("LocalizationManager: Could not find \(file).json for locale \(locale) in bundle.")
            }
        }
        
        self.strings = loadedStrings
    }
    
    func string(_ key: String, args: [String: Any] = [:]) -> String {
        guard let format = strings[key] else {
            #if DEBUG
            return "[[missing.\(key)]]"
            #else
            return key // Fallback to key in production
            #endif
        }
        
        var result = format
        for (argKey, argValue) in args {
            let placeholder = "{\(argKey)}"
            result = result.replacingOccurrences(of: placeholder, with: "\(argValue)")
        }
        
        return result
    }
}
