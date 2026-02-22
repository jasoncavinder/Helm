import Foundation

extension String {
    var localized: String {
        return LocalizationManager.shared.string(self)
    }
    
    func localized(with args: [String: Any]) -> String {
        return LocalizationManager.shared.string(self, args: args)
    }
}
