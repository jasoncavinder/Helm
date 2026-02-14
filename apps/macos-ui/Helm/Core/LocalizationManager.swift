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
    
    // Remove setLocale since setting property handles it
    
    private func loadLocale(_ locale: String) {
        // For now, only 'en' is supported, but structure is ready for others.
        // We look for files in the 'locales/{locale}' subdirectory of the main bundle.
        // Files to load: common.json, app.json, service.json
        
        var loadedStrings: [String: String] = [:]
        let files = ["common", "app", "service"]
        
        for file in files {
            if let url = Bundle.main.url(forResource: file, withExtension: "json", subdirectory: "locales/\(locale)") {
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
        
        // Simple variable replacement: {name} -> value
        var result = format
        for (argKey, argValue) in args {
            let placeholder = "{\(argKey)}"
            result = result.replacingOccurrences(of: placeholder, with: "\(argValue)")
        }
        
        return result
    }
}
