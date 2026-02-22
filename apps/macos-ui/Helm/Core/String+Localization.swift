import Foundation

extension String {
    var localized: String {
        return LocalizationManager.shared.string(self)
    }
    
    func localized(with args: [String: Any]) -> String {
        return LocalizationManager.shared.string(self, args: args)
    }
}

func localizedManagerDisplayName(_ managerId: String) -> String {
    switch managerId.lowercased() {
    case "homebrew_formula": return L10n.App.Managers.Name.homebrew.localized
    case "homebrew_cask": return L10n.App.Managers.Name.homebrewCask.localized
    case "npm", "npm_global": return L10n.App.Managers.Name.npm.localized
    case "pnpm": return L10n.App.Managers.Name.pnpm.localized
    case "yarn": return L10n.App.Managers.Name.yarn.localized
    case "poetry": return L10n.App.Managers.Name.poetry.localized
    case "rubygems": return L10n.App.Managers.Name.rubygems.localized
    case "bundler": return L10n.App.Managers.Name.bundler.localized
    case "pip": return L10n.App.Managers.Name.pip.localized
    case "pipx": return L10n.App.Managers.Name.pipx.localized
    case "cargo": return L10n.App.Managers.Name.cargo.localized
    case "cargo_binstall": return L10n.App.Managers.Name.cargoBinstall.localized
    case "mise": return L10n.App.Managers.Name.mise.localized
    case "rustup": return L10n.App.Managers.Name.rustup.localized
    case "softwareupdate": return L10n.App.Managers.Name.softwareUpdate.localized
    case "mas": return L10n.App.Managers.Name.appStore.localized
    case "sparkle": return L10n.App.Managers.Name.sparkle.localized
    case "setapp": return L10n.App.Managers.Name.setapp.localized
    case "asdf": return L10n.App.Managers.Name.asdf.localized
    case "macports": return L10n.App.Managers.Name.macports.localized
    case "nix_darwin": return L10n.App.Managers.Name.nixDarwin.localized
    case "docker_desktop": return L10n.App.Managers.Name.dockerDesktop.localized
    case "podman": return L10n.App.Managers.Name.podman.localized
    case "colima": return L10n.App.Managers.Name.colima.localized
    case "parallels_desktop": return L10n.App.Managers.Name.parallelsDesktop.localized
    case "xcode_command_line_tools": return L10n.App.Managers.Name.xcodeCommandLineTools.localized
    case "rosetta2": return L10n.App.Managers.Name.rosetta2.localized
    case "firmware_updates": return L10n.App.Managers.Name.firmwareUpdates.localized
    default:
        if let manager = ManagerInfo.find(byId: managerId) {
            return manager.displayName
        }
        return managerId.replacingOccurrences(of: "_", with: " ").capitalized
    }
}
