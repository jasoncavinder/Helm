import AppKit
import SwiftUI

final class HelmSettingsOpenRouter {
    typealias ActivateAction = () -> Void
    typealias FallbackOpenAction = () -> Void

    private struct Registration {
        let id: UUID
        let action: () -> Void
    }

    private let activateApp: ActivateAction
    private let fallbackOpenSettings: FallbackOpenAction
    private var registrations: [Registration] = []
    private var hasPendingRequest = false

    init(
        activateApp: @escaping ActivateAction = {
            NSApp.activate(ignoringOtherApps: true)
        },
        fallbackOpenSettings: @escaping FallbackOpenAction = {
            if !NSApp.sendAction(Selector(("showSettingsWindow:")), to: nil, from: nil) {
                NSApp.sendAction(Selector(("showPreferencesWindow:")), to: nil, from: nil)
            }
        }
    ) {
        self.activateApp = activateApp
        self.fallbackOpenSettings = fallbackOpenSettings
    }

    @discardableResult
    func register(_ action: @escaping () -> Void) -> UUID {
        let registration = Registration(id: UUID(), action: action)
        registrations.append(registration)
        if hasPendingRequest {
            hasPendingRequest = false
            DispatchQueue.main.async(execute: action)
        }
        return registration.id
    }

    func unregister(_ id: UUID) {
        registrations.removeAll { $0.id == id }
    }

    func requestOpen() {
        activateApp()
        if #available(macOS 14.0, *) {
            if !requestRegisteredOpen(recordPendingRequest: false) {
                fallbackOpenSettings()
            }
        } else {
            fallbackOpenSettings()
        }
    }

    func requestRegisteredOpen() {
        _ = requestRegisteredOpen(recordPendingRequest: true)
    }

    @discardableResult
    private func requestRegisteredOpen(recordPendingRequest: Bool) -> Bool {
        guard let action = registrations.last?.action else {
            if recordPendingRequest {
                hasPendingRequest = true
            }
            return false
        }
        action()
        return true
    }
}

struct HelmSettingsOpeningBridge: View {
    let router: HelmSettingsOpenRouter

    @ViewBuilder
    var body: some View {
        if #available(macOS 14.0, *) {
            ModernSettingsOpeningBridge(router: router)
        } else {
            EmptyView()
        }
    }
}

@available(macOS 14.0, *)
private struct ModernSettingsOpeningBridge: View {
    @Environment(\.openSettings) private var openSettings
    let router: HelmSettingsOpenRouter

    var body: some View {
        SettingsOpeningRegistrationView(
            router: router,
            openAction: { openSettings() }
        )
            .frame(width: 0, height: 0)
            .accessibilityHidden(true)
    }
}

@available(macOS 14.0, *)
private struct SettingsOpeningRegistrationView: NSViewRepresentable {
    let router: HelmSettingsOpenRouter
    let openAction: () -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(router: router, openAction: openAction)
    }

    func makeNSView(context: Context) -> NSView {
        context.coordinator.registerIfNeeded()
        return NSView(frame: .zero)
    }

    func updateNSView(_ view: NSView, context: Context) {
        context.coordinator.openAction = openAction
        context.coordinator.registerIfNeeded()
    }

    static func dismantleNSView(_ view: NSView, coordinator: Coordinator) {
        coordinator.unregister()
    }

    final class Coordinator {
        private let router: HelmSettingsOpenRouter
        private var registrationID: UUID?
        var openAction: () -> Void

        init(router: HelmSettingsOpenRouter, openAction: @escaping () -> Void) {
            self.router = router
            self.openAction = openAction
        }

        func registerIfNeeded() {
            guard registrationID == nil else { return }
            registrationID = router.register { [weak self] in
                self?.openAction()
            }
        }

        func unregister() {
            guard let registrationID else { return }
            router.unregister(registrationID)
            self.registrationID = nil
        }
    }
}
