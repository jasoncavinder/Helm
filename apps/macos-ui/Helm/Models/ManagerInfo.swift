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
    let isOptional: Bool
    let isDetectionOnly: Bool
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
        case "mise", "asdf", "rustup":
            return .authoritative
        case "homebrew_formula", "softwareupdate", "macports", "nix_darwin",
            "xcode_command_line_tools", "rosetta2", "firmware_updates":
            return .guarded
        default:
            return .standard
        }
    }

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
        [
            "homebrew_formula",
            "mise",
            "asdf",
            "rustup",
            "macports",
            "nix_darwin",
            "npm",
            "pnpm",
            "yarn",
            "pip",
            "pipx",
            "poetry",
            "rubygems",
            "bundler",
            "cargo",
            "cargo_binstall"
        ].contains(id)
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
        case "mise", "asdf", "rustup":
            return "wrench.and.screwdriver.fill"
        case "docker_desktop", "podman", "colima":
            return "shippingbox.circle.fill"
        case "parallels_desktop":
            return "desktopcomputer"
        case "xcode_command_line_tools":
            return "hammer.fill"
        case "rosetta2":
            return "translate"
        case "firmware_updates":
            return "memorychip.fill"
        default:
            return "shippingbox.fill"
        }
    }

    static let all: [ManagerInfo] = [
        // Toolchain / Runtime
        ManagerInfo(id: "mise", displayName: "mise", shortName: "mise", category: "Toolchain", isImplemented: true, isOptional: false, isDetectionOnly: false, installMethod: .automatable),
        ManagerInfo(id: "asdf", displayName: "asdf", shortName: "asdf", category: "Toolchain", isImplemented: true, isOptional: true, isDetectionOnly: false, installMethod: .notManageable),
        ManagerInfo(id: "rustup", displayName: "rustup", shortName: "rustup", category: "Toolchain", isImplemented: true, isOptional: false, isDetectionOnly: false, installMethod: .updateAndUninstall),

        // System / OS
        ManagerInfo(id: "homebrew_formula", displayName: "Homebrew (formulae)", shortName: "brew", category: "System/OS", isImplemented: true, isOptional: false, isDetectionOnly: false, installMethod: .updateOnly),
        ManagerInfo(id: "softwareupdate", displayName: "Software Update", shortName: "swupd", category: "System/OS", isImplemented: true, isOptional: false, isDetectionOnly: false, installMethod: .systemBinary),
        ManagerInfo(id: "macports", displayName: "MacPorts", shortName: "port", category: "System/OS", isImplemented: true, isOptional: true, isDetectionOnly: false, installMethod: .notManageable),
        ManagerInfo(id: "nix_darwin", displayName: "nix-darwin", shortName: "nix", category: "System/OS", isImplemented: true, isOptional: true, isDetectionOnly: false, installMethod: .notManageable),

        // Language
        ManagerInfo(id: "npm", displayName: "npm (global)", shortName: "npm", category: "Language", isImplemented: true, isOptional: false, isDetectionOnly: false, installMethod: .notManageable),
        ManagerInfo(id: "pnpm", displayName: "pnpm (global)", shortName: "pnpm", category: "Language", isImplemented: true, isOptional: false, isDetectionOnly: false, installMethod: .notManageable),
        ManagerInfo(id: "yarn", displayName: "yarn (global)", shortName: "yarn", category: "Language", isImplemented: true, isOptional: false, isDetectionOnly: false, installMethod: .notManageable),
        ManagerInfo(id: "poetry", displayName: "Poetry", shortName: "poetry", category: "Language", isImplemented: true, isOptional: false, isDetectionOnly: false, installMethod: .notManageable),
        ManagerInfo(id: "rubygems", displayName: "RubyGems", shortName: "gem", category: "Language", isImplemented: true, isOptional: false, isDetectionOnly: false, installMethod: .notManageable),
        ManagerInfo(id: "bundler", displayName: "Bundler", shortName: "bundle", category: "Language", isImplemented: true, isOptional: false, isDetectionOnly: false, installMethod: .notManageable),
        ManagerInfo(id: "pip", displayName: "pip", shortName: "pip", category: "Language", isImplemented: true, isOptional: false, isDetectionOnly: false, installMethod: .notManageable),
        ManagerInfo(id: "pipx", displayName: "pipx", shortName: "pipx", category: "Language", isImplemented: true, isOptional: false, isDetectionOnly: false, installMethod: .notManageable),
        ManagerInfo(id: "cargo", displayName: "Cargo", shortName: "cargo", category: "Language", isImplemented: true, isOptional: false, isDetectionOnly: false, installMethod: .notManageable),
        ManagerInfo(id: "cargo_binstall", displayName: "cargo-binstall", shortName: "binstall", category: "Language", isImplemented: true, isOptional: false, isDetectionOnly: false, installMethod: .notManageable),

        // App / GUI
        ManagerInfo(id: "mas", displayName: "Mac App Store", shortName: "mas", category: "App Store", isImplemented: true, isOptional: false, isDetectionOnly: false, installMethod: .automatable),
        ManagerInfo(id: "sparkle", displayName: "Sparkle Updaters", shortName: "sparkle", category: "App Store", isImplemented: true, isOptional: false, isDetectionOnly: true, installMethod: .notManageable),
        ManagerInfo(id: "setapp", displayName: "Setapp", shortName: "setapp", category: "App Store", isImplemented: true, isOptional: false, isDetectionOnly: true, installMethod: .notManageable),
        ManagerInfo(id: "homebrew_cask", displayName: "Homebrew (casks)", shortName: "cask", category: "App Store", isImplemented: true, isOptional: false, isDetectionOnly: false, installMethod: .notManageable),

        // Container / VM
        ManagerInfo(id: "docker_desktop", displayName: "Docker Desktop", shortName: "docker", category: "Container/VM", isImplemented: true, isOptional: false, isDetectionOnly: false, installMethod: .notManageable),
        ManagerInfo(id: "podman", displayName: "podman", shortName: "podman", category: "Container/VM", isImplemented: true, isOptional: false, isDetectionOnly: false, installMethod: .notManageable),
        ManagerInfo(id: "colima", displayName: "colima", shortName: "colima", category: "Container/VM", isImplemented: true, isOptional: false, isDetectionOnly: false, installMethod: .notManageable),
        ManagerInfo(id: "parallels_desktop", displayName: "Parallels Desktop", shortName: "parallels", category: "Container/VM", isImplemented: true, isOptional: false, isDetectionOnly: true, installMethod: .notManageable),

        // Security / Firmware
        ManagerInfo(id: "xcode_command_line_tools", displayName: "Xcode Command Line Tools", shortName: "xcode", category: "Security/Firmware", isImplemented: true, isOptional: false, isDetectionOnly: false, installMethod: .notManageable),
        ManagerInfo(id: "rosetta2", displayName: "Rosetta 2", shortName: "rosetta", category: "Security/Firmware", isImplemented: true, isOptional: false, isDetectionOnly: false, installMethod: .notManageable),
        ManagerInfo(id: "firmware_updates", displayName: "Firmware Updates", shortName: "firmware", category: "Security/Firmware", isImplemented: true, isOptional: false, isDetectionOnly: false, installMethod: .systemBinary),
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

    // Category ordering matching core registry.
    private static let categoryOrder: [String] = [
        "Toolchain", "System/OS", "Language", "App Store", "Container/VM", "Security/Firmware"
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
