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
    var summary: String? = nil
    var restartRequired: Bool = false
    private var statusOverride: PackageStatus? = nil

    var status: PackageStatus {
        if let override_ = statusOverride { return override_ }
        return latestVersion != nil ? .upgradable : .installed
    }

    init(id: String, name: String, version: String, latestVersion: String? = nil, manager: String, summary: String? = nil, restartRequired: Bool = false, status: PackageStatus? = nil) {
        self.id = id
        self.name = name
        self.version = version
        self.latestVersion = latestVersion
        self.manager = manager
        self.summary = summary
        self.restartRequired = restartRequired
        self.statusOverride = status
    }
}
