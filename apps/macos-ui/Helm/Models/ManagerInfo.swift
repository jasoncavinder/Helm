import Foundation

enum ManagerInstallMethod {
    case automatable          // Can install/update/uninstall via Helm (e.g., mise/mas via brew)
    case updateAndUninstall   // Can update/uninstall but not install (e.g., rustup)
    case updateOnly           // Can only self-update (e.g., Homebrew)
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
        installMethod == .automatable || installMethod == .updateAndUninstall
    }

    var canUpdate: Bool {
        installMethod == .automatable
            || installMethod == .updateAndUninstall
            || installMethod == .updateOnly
    }

    var authority: ManagerAuthority {
        switch id {
        case "mise", "rustup":
            return .authoritative
        case "homebrew_formula", "softwareupdate", "homebrew_cask":
            return .guarded
        default:
            return .standard
        }
    }

    // swiftlint:disable:next cyclomatic_complexity
    var capabilities: [String] {
        var result: [String] = [
            L10n.App.Capability.list,
            L10n.App.Capability.outdated,
        ]
        if canInstall {
            result.append(L10n.App.Capability.install)
        }
        if canUninstall {
            result.append(L10n.App.Capability.uninstall)
        }
        if canUpdate {
            result.append(L10n.App.Capability.upgrade)
        }
        if canSearch {
            result.append(L10n.App.Capability.search)
        }
        if canPin {
            result.append(L10n.App.Capability.pin)
        }
        return result
    }

    var canSearch: Bool {
        ["npm", "pnpm", "yarn", "pip", "cargo", "cargo_binstall", "poetry", "rubygems", "bundler"].contains(id)
    }

    var canPin: Bool {
        id == "homebrew_formula"
    }

    var symbolName: String {
        switch id {
        case "softwareupdate":
            return "apple.logo"
        case "homebrew_formula", "homebrew_cask":
            return "cup.and.saucer.fill"
        case "mise", "rustup":
            return "wrench.and.screwdriver.fill"
        default:
            return "shippingbox.fill"
        }
    }

    static let all: [ManagerInfo] = [
        ManagerInfo(id: "homebrew_formula", displayName: "Homebrew (formulae)", shortName: "brew", category: "System/OS", isImplemented: true, installMethod: .updateOnly),
        ManagerInfo(id: "homebrew_cask", displayName: "Homebrew (casks)", shortName: "cask", category: "App Store", isImplemented: false, installMethod: .notManageable),
        ManagerInfo(id: "npm", displayName: "npm (global)", shortName: "npm", category: "Language", isImplemented: true, installMethod: .notManageable),
        ManagerInfo(id: "pnpm", displayName: "pnpm (global)", shortName: "pnpm", category: "Language", isImplemented: true, installMethod: .notManageable),
        ManagerInfo(id: "yarn", displayName: "yarn (global)", shortName: "yarn", category: "Language", isImplemented: true, installMethod: .notManageable),
        ManagerInfo(id: "poetry", displayName: "Poetry", shortName: "poetry", category: "Language", isImplemented: true, installMethod: .notManageable),
        ManagerInfo(id: "rubygems", displayName: "RubyGems", shortName: "gem", category: "Language", isImplemented: true, installMethod: .notManageable),
        ManagerInfo(id: "bundler", displayName: "Bundler", shortName: "bundle", category: "Language", isImplemented: true, installMethod: .notManageable),
        ManagerInfo(id: "pip", displayName: "pip", shortName: "pip", category: "Language", isImplemented: true, installMethod: .notManageable),
        ManagerInfo(id: "pipx", displayName: "pipx", shortName: "pipx", category: "Language", isImplemented: true, installMethod: .notManageable),
        ManagerInfo(id: "cargo", displayName: "Cargo", shortName: "cargo", category: "Language", isImplemented: true, installMethod: .notManageable),
        ManagerInfo(id: "cargo_binstall", displayName: "cargo-binstall", shortName: "binstall", category: "Language", isImplemented: true, installMethod: .notManageable),
        ManagerInfo(id: "mise", displayName: "mise", shortName: "mise", category: "Toolchain", isImplemented: true, installMethod: .automatable),
        ManagerInfo(id: "rustup", displayName: "rustup", shortName: "rustup", category: "Toolchain", isImplemented: true, installMethod: .updateAndUninstall),
        ManagerInfo(id: "softwareupdate", displayName: "Software Update", shortName: "swupd", category: "System/OS", isImplemented: true, installMethod: .systemBinary),
        ManagerInfo(id: "mas", displayName: "Mac App Store", shortName: "mas", category: "App Store", isImplemented: true, installMethod: .automatable),
    ]

    static func find(byId managerId: String) -> ManagerInfo? {
        all.first { $0.id == managerId }
    }

    static func find(byDisplayName displayName: String) -> ManagerInfo? {
        all.first { localizedManagerDisplayName($0.id) == displayName }
    }

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
