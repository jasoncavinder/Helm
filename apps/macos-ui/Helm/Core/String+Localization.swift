import Foundation

extension String {
    var localized: String {
        return LocalizationManager.shared.string(self)
    }
    
    func localized(with args: [String: Any]) -> String {
        return LocalizationManager.shared.string(self, args: args)
    }
}

private enum ManagerDisplayNameKeys {
    // Cache keys only; resolving each call keeps language changes live.
    static let byID: [String: String] = [
        "homebrew_formula": L10n.App.Managers.Name.homebrew,
        "homebrew_cask": L10n.App.Managers.Name.homebrewCask,
        "npm": L10n.App.Managers.Name.npm,
        "npm_global": L10n.App.Managers.Name.npm,
        "pnpm": L10n.App.Managers.Name.pnpm,
        "yarn": L10n.App.Managers.Name.yarn,
        "poetry": L10n.App.Managers.Name.poetry,
        "rubygems": L10n.App.Managers.Name.rubygems,
        "bundler": L10n.App.Managers.Name.bundler,
        "pip": L10n.App.Managers.Name.pip,
        "pipx": L10n.App.Managers.Name.pipx,
        "uv": L10n.App.Managers.Name.uv,
        "cargo": L10n.App.Managers.Name.cargo,
        "cargo_binstall": L10n.App.Managers.Name.cargoBinstall,
        "mise": L10n.App.Managers.Name.mise,
        "rustup": L10n.App.Managers.Name.rustup,
        "softwareupdate": L10n.App.Managers.Name.softwareUpdate,
        "mas": L10n.App.Managers.Name.appStore,
        "sparkle": L10n.App.Managers.Name.sparkle,
        HelmCore.helmSelfUpdateManagerId: L10n.App.Updates.helmSelfUpdateManager,
        "setapp": L10n.App.Managers.Name.setapp,
        "asdf": L10n.App.Managers.Name.asdf,
        "macports": L10n.App.Managers.Name.macports,
        "nix_darwin": L10n.App.Managers.Name.nixDarwin,
        "docker_desktop": L10n.App.Managers.Name.dockerDesktop,
        "podman": L10n.App.Managers.Name.podman,
        "colima": L10n.App.Managers.Name.colima,
        "parallels_desktop": L10n.App.Managers.Name.parallelsDesktop,
        "xcode_command_line_tools": L10n.App.Managers.Name.xcodeCommandLineTools,
        "rosetta2": L10n.App.Managers.Name.rosetta2,
        "firmware_updates": L10n.App.Managers.Name.firmwareUpdates
    ]
}

func localizedManagerDisplayName(_ managerId: String) -> String {
    if let key = ManagerDisplayNameKeys.byID[managerId.lowercased()] {
        return key.localized
    }
    if let manager = ManagerInfo.find(byId: managerId) {
        return manager.displayName
    }
    return managerId.replacingOccurrences(of: "_", with: " ").capitalized
}

extension ManagerDistributionMethod {
    var localizedName: String {
        switch self {
        case .homebrew: return L10n.App.Inspector.InstallMethod.homebrew.localized
        case .macports: return L10n.App.Inspector.InstallMethod.macports.localized
        case .appStore: return L10n.App.Inspector.InstallMethod.appStore.localized
        case .setapp: return L10n.App.Inspector.InstallMethod.setapp.localized
        case .officialInstaller: return L10n.App.Inspector.InstallMethod.officialInstaller.localized
        case .scriptInstaller: return L10n.App.Inspector.InstallMethod.scriptInstaller.localized
        case .corepack: return L10n.App.Inspector.InstallMethod.corepack.localized
        case .rustupInstaller: return L10n.App.Inspector.InstallMethod.rustupInstaller.localized
        case .xcodeSelect: return L10n.App.Inspector.InstallMethod.xcodeSelect.localized
        case .softwareUpdate: return L10n.App.Inspector.InstallMethod.softwareUpdate.localized
        case .systemProvided: return L10n.App.Inspector.InstallMethod.systemProvided.localized
        case .npm: return L10n.App.Inspector.InstallMethod.npm.localized
        case .pip: return L10n.App.Inspector.InstallMethod.pip.localized
        case .pipx: return L10n.App.Inspector.InstallMethod.pipx.localized
        case .gem: return L10n.App.Inspector.InstallMethod.gem.localized
        case .cargoInstall: return L10n.App.Inspector.InstallMethod.cargoInstall.localized
        case .asdf: return L10n.App.Inspector.InstallMethod.asdf.localized
        case .mise: return L10n.App.Inspector.InstallMethod.mise.localized
        case .notManageable: return L10n.App.Inspector.InstallMethod.notManageable.localized
        }
    }
}

enum ManagerDependencyResolver {
    static func dependencyManagerId(for managerId: String, provenance: String?) -> String? {
        let normalized = normalizedProvenance(provenance)
        switch normalized {
        case "homebrew":
            return "homebrew_formula"
        case "macports":
            return "macports"
        case "nix":
            return "nix_darwin"
        case "asdf":
            return "asdf"
        case "mise":
            return managerId == "mise" ? nil : "mise"
        default:
            return nil
        }
    }

    static func dependencyManagerId(
        for managerId: String,
        installMethod: ManagerDistributionMethod
    ) -> String? {
        switch installMethod {
        case .homebrew:
            return "homebrew_formula"
        case .macports:
            return "macports"
        case .npm:
            return "npm"
        case .pip:
            return "pip"
        case .pipx:
            return "pipx"
        case .gem:
            return "rubygems"
        case .cargoInstall:
            return "cargo"
        case .asdf:
            return "asdf"
        case .mise:
            return managerId == "mise" ? nil : "mise"
        case .rustupInstaller:
            return managerId == "rustup" ? nil : "rustup"
        case .appStore, .setapp, .officialInstaller, .scriptInstaller, .corepack,
             .xcodeSelect, .softwareUpdate, .systemProvided, .notManageable:
            return nil
        }
    }

    static func enabledDependents(
        of managerId: String,
        statuses: [String: ManagerStatus]
    ) -> [String] {
        statuses.values
            .filter { status in
                status.managerId != managerId &&
                    status.enabled &&
                    dependencyManagerId(for: status.managerId, provenance: status.activeProvenance) == managerId
            }
            .map(\.managerId)
            .sorted()
    }

    private static func normalizedProvenance(_ value: String?) -> String {
        value?
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased() ?? ""
    }
}
