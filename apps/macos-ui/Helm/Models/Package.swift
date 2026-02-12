import Foundation

struct PackageItem: Identifiable {
    let id: String
    let name: String
    let version: String
    var latestVersion: String? = nil
    let manager: String
}
