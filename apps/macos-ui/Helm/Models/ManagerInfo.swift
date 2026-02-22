import Foundation

enum ManagerInstallMethod {
    case automatable          // Can install/update/uninstall via Helm (e.g., mise/mas via brew)
    case updateAndUninstall   // Can update/uninstall but not install (e.g., rustup)
    case updateOnly           // Can only self-update (e.g., Homebrew)
    case systemBinary         // Always present, not manageable (e.g., softwareupdate)
    case notManageable        // Not automatable at all
}

enum ManagerDistributionMethod: String, CaseIterable {
    case homebrew
    case macports
    case appStore
    case setapp
    case officialInstaller
    case scriptInstaller
    case corepack
    case rustupInstaller
    case xcodeSelect
    case softwareUpdate
    case systemProvided
    case npm
    case pip
    case pipx
    case gem
    case cargoInstall
    case asdf
    case mise
    case notManageable

    var displayName: String {
        switch self {
        case .homebrew:
            return "Homebrew"
        case .macports:
            return "MacPorts"
        case .appStore:
            return "Mac App Store"
        case .setapp:
            return "Setapp"
        case .officialInstaller:
            return "Official Installer"
        case .scriptInstaller:
            return "Script Installer"
        case .corepack:
            return "Corepack"
        case .rustupInstaller:
            return "rustup-init"
        case .xcodeSelect:
            return "xcode-select"
        case .softwareUpdate:
            return "softwareupdate"
        case .systemProvided:
            return "System Provided"
        case .npm:
            return "npm"
        case .pip:
            return "pip"
        case .pipx:
            return "pipx"
        case .gem:
            return "RubyGems"
        case .cargoInstall:
            return "cargo install"
        case .asdf:
            return "asdf"
        case .mise:
            return "mise"
        case .notManageable:
            return "Not Manageable"
        }
    }
}

struct ManagerInstallMethodOption: Identifiable, Equatable {
    let method: ManagerDistributionMethod
    let isRecommended: Bool
    let isPreferred: Bool
    let executablePathHints: [String]
    let packageHints: [String]

    var id: String { method.rawValue }
}

private func methodOption(
    _ method: ManagerDistributionMethod,
    recommended: Bool = false,
    preferred: Bool = false,
    executablePathHints: [String] = [],
    packageHints: [String] = []
) -> ManagerInstallMethodOption {
    ManagerInstallMethodOption(
        method: method,
        isRecommended: recommended,
        isPreferred: preferred,
        executablePathHints: executablePathHints,
        packageHints: packageHints
    )
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
    let installMethodOptions: [ManagerInstallMethodOption]

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
            "asdf",
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

    func selectedInstallMethodOption(
        executablePath: String?,
        installedPackages: [PackageItem]
    ) -> ManagerInstallMethodOption {
        if let executablePath {
            let normalizedPath = executablePath.lowercased()
            if let pathMatch = installMethodOptions.first(where: { option in
                option.executablePathHints.contains(where: { hint in
                    normalizedPath.contains(hint.lowercased())
                })
            }) {
                return pathMatch
            }
        }

        let installedPackageNames = Set(installedPackages.map { $0.name.lowercased() })
        if let packageMatch = installMethodOptions.first(where: { option in
            option.packageHints.contains(where: { installedPackageNames.contains($0.lowercased()) })
        }) {
            return packageMatch
        }

        if let recommended = installMethodOptions.first(where: { $0.isRecommended }) {
            return recommended
        }
        if let preferred = installMethodOptions.first(where: { $0.isPreferred }) {
            return preferred
        }
        return installMethodOptions.first ?? methodOption(.notManageable)
    }

    static let all: [ManagerInfo] = [
        // Toolchain / Runtime
        ManagerInfo(
            id: "mise",
            displayName: "mise",
            shortName: "mise",
            category: "Toolchain",
            isImplemented: true,
            isOptional: false,
            isDetectionOnly: false,
            installMethod: .automatable,
            installMethodOptions: [
                methodOption(
                    .homebrew,
                    recommended: true,
                    preferred: true,
                    executablePathHints: ["/opt/homebrew/bin/mise", "/usr/local/bin/mise"],
                    packageHints: ["mise"]
                ),
                methodOption(.scriptInstaller, executablePathHints: [".local/bin/mise"]),
                methodOption(.macports),
                methodOption(.cargoInstall)
            ]
        ),
        ManagerInfo(
            id: "asdf",
            displayName: "asdf",
            shortName: "asdf",
            category: "Toolchain",
            isImplemented: true,
            isOptional: true,
            isDetectionOnly: false,
            installMethod: .notManageable,
            installMethodOptions: [
                methodOption(
                    .scriptInstaller,
                    recommended: true,
                    preferred: true,
                    executablePathHints: [".asdf/bin/asdf"]
                ),
                methodOption(
                    .homebrew,
                    executablePathHints: ["/opt/homebrew/bin/asdf", "/usr/local/bin/asdf"],
                    packageHints: ["asdf"]
                )
            ]
        ),
        ManagerInfo(
            id: "rustup",
            displayName: "rustup",
            shortName: "rustup",
            category: "Toolchain",
            isImplemented: true,
            isOptional: false,
            isDetectionOnly: false,
            installMethod: .updateAndUninstall,
            installMethodOptions: [
                methodOption(
                    .rustupInstaller,
                    recommended: true,
                    preferred: true,
                    executablePathHints: [".cargo/bin/rustup"]
                ),
                methodOption(
                    .homebrew,
                    executablePathHints: ["/opt/homebrew/bin/rustup", "/usr/local/bin/rustup"],
                    packageHints: ["rustup"]
                )
            ]
        ),

        // System / OS
        ManagerInfo(
            id: "homebrew_formula",
            displayName: "Homebrew (formulae)",
            shortName: "brew",
            category: "System/OS",
            isImplemented: true,
            isOptional: false,
            isDetectionOnly: false,
            installMethod: .updateOnly,
            installMethodOptions: [
                methodOption(
                    .homebrew,
                    recommended: true,
                    preferred: true,
                    executablePathHints: ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"],
                    packageHints: ["homebrew"]
                ),
                methodOption(.scriptInstaller)
            ]
        ),
        ManagerInfo(
            id: "softwareupdate",
            displayName: "Software Update",
            shortName: "swupd",
            category: "System/OS",
            isImplemented: true,
            isOptional: false,
            isDetectionOnly: false,
            installMethod: .systemBinary,
            installMethodOptions: [
                methodOption(
                    .softwareUpdate,
                    recommended: true,
                    preferred: true,
                    executablePathHints: ["/usr/sbin/softwareupdate"]
                )
            ]
        ),
        ManagerInfo(
            id: "macports",
            displayName: "MacPorts",
            shortName: "port",
            category: "System/OS",
            isImplemented: true,
            isOptional: true,
            isDetectionOnly: false,
            installMethod: .notManageable,
            installMethodOptions: [
                methodOption(
                    .macports,
                    recommended: true,
                    preferred: true,
                    executablePathHints: ["/opt/local/bin/port"]
                ),
                methodOption(.officialInstaller)
            ]
        ),
        ManagerInfo(
            id: "nix_darwin",
            displayName: "nix-darwin",
            shortName: "nix",
            category: "System/OS",
            isImplemented: true,
            isOptional: true,
            isDetectionOnly: false,
            installMethod: .notManageable,
            installMethodOptions: [
                methodOption(
                    .scriptInstaller,
                    recommended: true,
                    preferred: true,
                    executablePathHints: ["darwin-rebuild", "/nix/store"]
                ),
                methodOption(.homebrew, executablePathHints: ["/opt/homebrew/bin/darwin-rebuild"])
            ]
        ),

        // Language
        ManagerInfo(
            id: "npm",
            displayName: "npm (global)",
            shortName: "npm",
            category: "Language",
            isImplemented: true,
            isOptional: false,
            isDetectionOnly: false,
            installMethod: .notManageable,
            installMethodOptions: [
                methodOption(.mise, recommended: true, preferred: true),
                methodOption(.asdf),
                methodOption(.homebrew, packageHints: ["node"]),
                methodOption(.officialInstaller)
            ]
        ),
        ManagerInfo(
            id: "pnpm",
            displayName: "pnpm (global)",
            shortName: "pnpm",
            category: "Language",
            isImplemented: true,
            isOptional: false,
            isDetectionOnly: false,
            installMethod: .notManageable,
            installMethodOptions: [
                methodOption(.corepack, recommended: true, preferred: true),
                methodOption(.homebrew, packageHints: ["pnpm"]),
                methodOption(.npm),
                methodOption(.scriptInstaller)
            ]
        ),
        ManagerInfo(
            id: "yarn",
            displayName: "yarn (global)",
            shortName: "yarn",
            category: "Language",
            isImplemented: true,
            isOptional: false,
            isDetectionOnly: false,
            installMethod: .notManageable,
            installMethodOptions: [
                methodOption(.corepack, recommended: true, preferred: true),
                methodOption(.homebrew, packageHints: ["yarn"]),
                methodOption(.npm),
                methodOption(.scriptInstaller)
            ]
        ),
        ManagerInfo(
            id: "poetry",
            displayName: "Poetry",
            shortName: "poetry",
            category: "Language",
            isImplemented: true,
            isOptional: false,
            isDetectionOnly: false,
            installMethod: .notManageable,
            installMethodOptions: [
                methodOption(.pipx, recommended: true, preferred: true, packageHints: ["poetry"]),
                methodOption(.homebrew, packageHints: ["poetry"]),
                methodOption(.pip),
                methodOption(.officialInstaller)
            ]
        ),
        ManagerInfo(
            id: "rubygems",
            displayName: "RubyGems",
            shortName: "gem",
            category: "Language",
            isImplemented: true,
            isOptional: false,
            isDetectionOnly: false,
            installMethod: .notManageable,
            installMethodOptions: [
                methodOption(.systemProvided, recommended: true, preferred: true),
                methodOption(.homebrew),
                methodOption(.asdf),
                methodOption(.mise)
            ]
        ),
        ManagerInfo(
            id: "bundler",
            displayName: "Bundler",
            shortName: "bundle",
            category: "Language",
            isImplemented: true,
            isOptional: false,
            isDetectionOnly: false,
            installMethod: .notManageable,
            installMethodOptions: [
                methodOption(.gem, recommended: true, preferred: true),
                methodOption(.systemProvided),
                methodOption(.homebrew),
                methodOption(.asdf),
                methodOption(.mise)
            ]
        ),
        ManagerInfo(
            id: "pip",
            displayName: "pip",
            shortName: "pip",
            category: "Language",
            isImplemented: true,
            isOptional: false,
            isDetectionOnly: false,
            installMethod: .notManageable,
            installMethodOptions: [
                methodOption(.systemProvided, recommended: true, preferred: true),
                methodOption(.homebrew),
                methodOption(.asdf),
                methodOption(.mise)
            ]
        ),
        ManagerInfo(
            id: "pipx",
            displayName: "pipx",
            shortName: "pipx",
            category: "Language",
            isImplemented: true,
            isOptional: false,
            isDetectionOnly: false,
            installMethod: .notManageable,
            installMethodOptions: [
                methodOption(.homebrew, recommended: true, preferred: true, packageHints: ["pipx"]),
                methodOption(.pip)
            ]
        ),
        ManagerInfo(
            id: "cargo",
            displayName: "Cargo",
            shortName: "cargo",
            category: "Language",
            isImplemented: true,
            isOptional: false,
            isDetectionOnly: false,
            installMethod: .notManageable,
            installMethodOptions: [
                methodOption(.rustupInstaller, recommended: true, preferred: true),
                methodOption(.homebrew, packageHints: ["rust"])
            ]
        ),
        ManagerInfo(
            id: "cargo_binstall",
            displayName: "cargo-binstall",
            shortName: "binstall",
            category: "Language",
            isImplemented: true,
            isOptional: false,
            isDetectionOnly: false,
            installMethod: .notManageable,
            installMethodOptions: [
                methodOption(.scriptInstaller, recommended: true, preferred: true, packageHints: ["cargo-binstall"]),
                methodOption(.cargoInstall, packageHints: ["cargo-binstall"]),
                methodOption(.homebrew, packageHints: ["cargo-binstall"])
            ]
        ),

        // App / GUI
        ManagerInfo(
            id: "mas",
            displayName: "Mac App Store",
            shortName: "mas",
            category: "App Store",
            isImplemented: true,
            isOptional: false,
            isDetectionOnly: false,
            installMethod: .automatable,
            installMethodOptions: [
                methodOption(.homebrew, recommended: true, preferred: true, packageHints: ["mas"]),
                methodOption(.macports),
                methodOption(.appStore),
                methodOption(.officialInstaller)
            ]
        ),
        ManagerInfo(
            id: "sparkle",
            displayName: "Sparkle Updaters",
            shortName: "sparkle",
            category: "App Store",
            isImplemented: true,
            isOptional: false,
            isDetectionOnly: true,
            installMethod: .notManageable,
            installMethodOptions: [methodOption(.notManageable, recommended: true, preferred: true)]
        ),
        ManagerInfo(
            id: "setapp",
            displayName: "Setapp",
            shortName: "setapp",
            category: "App Store",
            isImplemented: true,
            isOptional: false,
            isDetectionOnly: true,
            installMethod: .notManageable,
            installMethodOptions: [
                methodOption(.setapp, recommended: true, preferred: true),
                methodOption(.notManageable)
            ]
        ),
        ManagerInfo(
            id: "homebrew_cask",
            displayName: "Homebrew (casks)",
            shortName: "cask",
            category: "App Store",
            isImplemented: true,
            isOptional: false,
            isDetectionOnly: false,
            installMethod: .notManageable,
            installMethodOptions: [
                methodOption(
                    .homebrew,
                    recommended: true,
                    preferred: true,
                    executablePathHints: ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"]
                )
            ]
        ),

        // Container / VM
        ManagerInfo(
            id: "docker_desktop",
            displayName: "Docker Desktop",
            shortName: "docker",
            category: "Container/VM",
            isImplemented: true,
            isOptional: false,
            isDetectionOnly: false,
            installMethod: .notManageable,
            installMethodOptions: [
                methodOption(.officialInstaller, recommended: true, preferred: true),
                methodOption(.homebrew, packageHints: ["docker-desktop"]),
                methodOption(.setapp)
            ]
        ),
        ManagerInfo(
            id: "podman",
            displayName: "podman",
            shortName: "podman",
            category: "Container/VM",
            isImplemented: true,
            isOptional: false,
            isDetectionOnly: false,
            installMethod: .notManageable,
            installMethodOptions: [
                methodOption(.officialInstaller, recommended: true, preferred: true),
                methodOption(.homebrew, packageHints: ["podman"]),
                methodOption(.macports)
            ]
        ),
        ManagerInfo(
            id: "colima",
            displayName: "colima",
            shortName: "colima",
            category: "Container/VM",
            isImplemented: true,
            isOptional: false,
            isDetectionOnly: false,
            installMethod: .notManageable,
            installMethodOptions: [
                methodOption(.homebrew, recommended: true, preferred: true, packageHints: ["colima"]),
                methodOption(.macports),
                methodOption(.mise)
            ]
        ),
        ManagerInfo(
            id: "parallels_desktop",
            displayName: "Parallels Desktop",
            shortName: "parallels",
            category: "Container/VM",
            isImplemented: true,
            isOptional: false,
            isDetectionOnly: true,
            installMethod: .notManageable,
            installMethodOptions: [
                methodOption(.officialInstaller, recommended: true, preferred: true),
                methodOption(.setapp),
                methodOption(.notManageable)
            ]
        ),

        // Security / Firmware
        ManagerInfo(
            id: "xcode_command_line_tools",
            displayName: "Xcode Command Line Tools",
            shortName: "xcode",
            category: "Security/Firmware",
            isImplemented: true,
            isOptional: false,
            isDetectionOnly: false,
            installMethod: .notManageable,
            installMethodOptions: [
                methodOption(.xcodeSelect, recommended: true, preferred: true),
                methodOption(.appStore)
            ]
        ),
        ManagerInfo(
            id: "rosetta2",
            displayName: "Rosetta 2",
            shortName: "rosetta",
            category: "Security/Firmware",
            isImplemented: true,
            isOptional: false,
            isDetectionOnly: false,
            installMethod: .notManageable,
            installMethodOptions: [
                methodOption(.softwareUpdate, recommended: true, preferred: true)
            ]
        ),
        ManagerInfo(
            id: "firmware_updates",
            displayName: "Firmware Updates",
            shortName: "firmware",
            category: "Security/Firmware",
            isImplemented: true,
            isOptional: false,
            isDetectionOnly: false,
            installMethod: .systemBinary,
            installMethodOptions: [
                methodOption(.systemProvided, recommended: true, preferred: true)
            ]
        ),
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

    private static let defaultPriorityByAuthority: [ManagerAuthority: [String]] = [
        .authoritative: [
            "mise",
            "rustup",
            "asdf"
        ],
        .standard: [
            "mas",
            "homebrew_cask",
            "npm",
            "pnpm",
            "yarn",
            "pipx",
            "pip",
            "poetry",
            "cargo",
            "cargo_binstall",
            "rubygems",
            "bundler",
            "docker_desktop",
            "colima",
            "podman",
            "sparkle",
            "setapp",
            "parallels_desktop"
        ],
        .guarded: [
            "homebrew_formula",
            "macports",
            "nix_darwin",
            "xcode_command_line_tools",
            "rosetta2",
            "firmware_updates",
            "softwareupdate"
        ]
    ]

    static func defaultPriorityRank(for managerId: String) -> Int {
        if let manager = find(byId: managerId),
           let order = defaultPriorityByAuthority[manager.authority],
           let index = order.firstIndex(of: managerId) {
            return index
        }
        return Int.max / 2
    }

    static func defaultPriorityOrder(for authority: ManagerAuthority) -> [String] {
        defaultPriorityByAuthority[authority] ?? []
    }

    static var groupedByCategory: [(category: String, managers: [ManagerInfo])] {
        var groups: [String: [ManagerInfo]] = [:]
        for manager in all {
            groups[manager.category, default: []].append(manager)
        }
        // Sort managers alphabetically within each group
        for key in groups.keys {
            groups[key]?.sort {
                $0.displayName.localizedCaseInsensitiveCompare($1.displayName) == .orderedAscending
            }
        }
        // Return groups in documented order, skip empty
        return categoryOrder.compactMap { cat in
            guard let managers = groups[cat], !managers.isEmpty else { return nil }
            return (category: cat, managers: managers)
        }
    }
}
