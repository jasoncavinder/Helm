import Foundation

struct ManagerInfo: Identifiable {
    let id: String
    let displayName: String
    let shortName: String
    let category: String
    let isImplemented: Bool

    var firstLetter: String {
        String(shortName.prefix(1)).uppercased()
    }

    static let all: [ManagerInfo] = [
        ManagerInfo(id: "homebrew_formula", displayName: "Homebrew (formulae)", shortName: "brew", category: "System/OS", isImplemented: true),
        ManagerInfo(id: "homebrew_cask", displayName: "Homebrew (casks)", shortName: "cask", category: "App Store", isImplemented: false),
        ManagerInfo(id: "npm_global", displayName: "npm (global)", shortName: "npm", category: "Language", isImplemented: false),
        ManagerInfo(id: "pipx", displayName: "pipx", shortName: "pipx", category: "Language", isImplemented: false),
        ManagerInfo(id: "cargo", displayName: "Cargo", shortName: "cargo", category: "Language", isImplemented: false),
        ManagerInfo(id: "mise", displayName: "mise", shortName: "mise", category: "Toolchain", isImplemented: true),
        ManagerInfo(id: "rustup", displayName: "rustup", shortName: "rustup", category: "Toolchain", isImplemented: true),
        ManagerInfo(id: "mas", displayName: "Mac App Store", shortName: "mas", category: "App Store", isImplemented: false),
    ]

    static var implemented: [ManagerInfo] {
        all.filter { $0.isImplemented }
    }
}
