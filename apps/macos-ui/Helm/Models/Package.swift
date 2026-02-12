import SwiftUI

enum PackageStatus: String, CaseIterable {
    case installed
    case upgradable
    case available

    var displayName: String {
        switch self {
        case .installed:  return "Installed"
        case .upgradable: return "Upgradable"
        case .available:  return "Available"
        }
    }

    var iconName: String {
        switch self {
        case .installed:  return "checkmark.circle.fill"
        case .upgradable: return "arrow.up.circle.fill"
        case .available:  return "plus.circle.fill"
        }
    }

    var iconColor: Color {
        switch self {
        case .installed:  return .green
        case .upgradable: return .orange
        case .available:  return .blue
        }
    }
}

struct PackageItem: Identifiable {
    let id: String
    let name: String
    let version: String
    var latestVersion: String? = nil
    let manager: String

    var status: PackageStatus {
        latestVersion != nil ? .upgradable : .installed
    }
}
