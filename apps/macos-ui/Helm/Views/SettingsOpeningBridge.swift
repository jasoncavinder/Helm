import AppKit

enum HelmPanelDeactivationPolicy {
    static let popoverHidesOnDeactivate = false
    static let settingsHidesOnDeactivate = false
}

enum HelmSettingsPanelPolicy {
    static let titleVisibility: NSWindow.TitleVisibility = .hidden
    static let styleMask: NSWindow.StyleMask = [
        .titled,
        .closable,
        .resizable,
        .fullSizeContentView
    ]
    static let collectionBehavior: NSWindow.CollectionBehavior = [
        .moveToActiveSpace,
        .transient,
        .canJoinAllApplications,
        .fullScreenAuxiliary
    ]
}

final class HelmSettingsOpenRouter {
    typealias OpenAction = () -> Void

    private var openAction: OpenAction

    init(openAction: @escaping OpenAction = {}) {
        self.openAction = openAction
    }

    func configure(openAction: @escaping OpenAction) {
        self.openAction = openAction
    }

    func requestOpen() {
        openAction()
    }
}
