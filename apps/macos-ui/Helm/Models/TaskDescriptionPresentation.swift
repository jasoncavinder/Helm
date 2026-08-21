import Foundation

struct TaskDescriptionPresentation {
    let rawDescription: String
    let labelKey: String?
    let labelArgs: [String: String]?

    func resolve(
        using resolver: (_ key: String, _ arguments: [String: String]) -> String
    ) -> String {
        guard let labelKey,
              !labelKey.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            return rawDescription
        }
        return resolver(labelKey, labelArgs ?? [:])
    }
}
