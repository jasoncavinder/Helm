import Foundation

enum ManagerInstallMethod {
    case automatable          // Can install/uninstall via Helm (e.g., mas via brew)
    case uninstallOnly        // Can only uninstall (e.g., rustup self uninstall)
    case systemBinary         // Always present, not manageable (e.g., softwareupdate)
    case notManageable        // Not automatable at all
}

struct ManagerInfo: Identifiable {
    let id: String
    let displayName: String
    let shortName: String
    let category: String
    let isImplemented: Bool
    let installMethod: ManagerInstallMethod

    var firstLetter: String {
        String(shortName.prefix(1)).uppercased()
    }

    var canInstall: Bool {
        installMethod == .automatable
    }

    var canUninstall: Bool {
        installMethod == .automatable || installMethod == .uninstallOnly
    }

    static let all: [ManagerInfo] = [
        ManagerInfo(id: "homebrew_formula", displayName: "Homebrew (formulae)", shortName: "brew", category: "System/OS", isImplemented: true, installMethod: .notManageable),
        ManagerInfo(id: "homebrew_cask", displayName: "Homebrew (casks)", shortName: "cask", category: "App Store", isImplemented: false, installMethod: .notManageable),
        ManagerInfo(id: "npm_global", displayName: "npm (global)", shortName: "npm", category: "Language", isImplemented: false, installMethod: .notManageable),
        ManagerInfo(id: "pipx", displayName: "pipx", shortName: "pipx", category: "Language", isImplemented: false, installMethod: .notManageable),
        ManagerInfo(id: "cargo", displayName: "Cargo", shortName: "cargo", category: "Language", isImplemented: false, installMethod: .notManageable),
        ManagerInfo(id: "mise", displayName: "mise", shortName: "mise", category: "Toolchain", isImplemented: true, installMethod: .automatable),
        ManagerInfo(id: "rustup", displayName: "rustup", shortName: "rustup", category: "Toolchain", isImplemented: true, installMethod: .uninstallOnly),
        ManagerInfo(id: "mas", displayName: "Mac App Store", shortName: "mas", category: "App Store", isImplemented: false, installMethod: .automatable),
    ]

    static var implemented: [ManagerInfo] {
        all.filter { $0.isImplemented }
    }

    // Category ordering matching core registry: ToolRuntime → SystemOs → Language → GuiApp
    private static let categoryOrder: [String] = [
        "Toolchain", "System/OS", "Language", "App Store"
    ]

    static var groupedByCategory: [(category: String, managers: [ManagerInfo])] {
        var groups: [String: [ManagerInfo]] = [:]
        for manager in all {
            groups[manager.category, default: []].append(manager)
        }
        // Sort managers alphabetically within each group
        for key in groups.keys {
            groups[key]?.sort { $0.displayName.localizedCaseInsensitiveCompare($1.displayName) == .orderedAscending }
        }
        // Return groups in documented order, skip empty
        return categoryOrder.compactMap { cat in
            guard let managers = groups[cat], !managers.isEmpty else { return nil }
            return (category: cat, managers: managers)
        }
    }
}
