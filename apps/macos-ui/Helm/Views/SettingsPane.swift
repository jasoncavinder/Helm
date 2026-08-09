enum SettingsPane: String, CaseIterable, Identifiable {
    case general
    case updates
    case sources
    case cli
    case support

    var id: String { rawValue }

    var titleKey: String {
        switch self {
        case .general:
            return "app.settings.section.general"
        case .updates:
            return "app.section.updates"
        case .sources:
            return "app.settings.pane.sources"
        case .cli:
            return "app.settings.cli.section"
        case .support:
            return "app.settings.support_feedback.section"
        }
    }

    var icon: String {
        switch self {
        case .general: return "gearshape"
        case .updates: return "arrow.triangle.2.circlepath"
        case .sources: return "point.3.connected.trianglepath.dotted"
        case .cli: return "terminal"
        case .support: return "lifepreserver"
        }
    }
}
