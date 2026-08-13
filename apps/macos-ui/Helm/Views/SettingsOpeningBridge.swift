import AppKit
import SwiftUI

enum HelmSettingsWindowSpacePolicy {
    static let requiredCollectionBehavior: NSWindow.CollectionBehavior = [
        .moveToActiveSpace,
        .canJoinAllApplications,
        .fullScreenAuxiliary
    ]

    static func normalizedCollectionBehavior(
        _ current: NSWindow.CollectionBehavior
    ) -> NSWindow.CollectionBehavior {
        var normalized = current
        normalized.remove(.canJoinAllSpaces)
        normalized.remove(.moveToActiveSpace)
        normalized.remove(.primary)
        normalized.remove(.auxiliary)
        normalized.remove(.fullScreenPrimary)
        normalized.remove(.fullScreenNone)
        normalized.formUnion(requiredCollectionBehavior)
        return normalized
    }

    @MainActor
    static func apply(to window: NSWindow) {
        let normalized = normalizedCollectionBehavior(window.collectionBehavior)
        guard normalized != window.collectionBehavior else { return }
        window.collectionBehavior = normalized
        window.makeKeyAndOrderFront(nil)
    }
}

struct HelmSettingsWindowSpaceBridge: NSViewRepresentable {
    func makeNSView(context: Context) -> SettingsWindowSpaceView {
        SettingsWindowSpaceView(frame: .zero)
    }

    func updateNSView(_ view: SettingsWindowSpaceView, context: Context) {
        view.applyPolicyToAttachedWindow()
    }
}

final class SettingsWindowSpaceView: NSView {
    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        applyPolicyToAttachedWindow()
    }

    func applyPolicyToAttachedWindow() {
        guard let window else { return }
        HelmSettingsWindowSpacePolicy.apply(to: window)
    }
}

final class HelmSettingsOpenRouter {
    typealias ActivateAction = () -> Void
    typealias VenturaOpenAction = () -> Void

    private struct Registration {
        let id: UUID
        let action: () -> Void
    }

    private let activateApp: ActivateAction
    private let openVenturaSettings: VenturaOpenAction
    private var registrations: [Registration] = []
    private var hasPendingRequest = false

    init(
        activateApp: @escaping ActivateAction = {
            NSApp.activate(ignoringOtherApps: true)
        },
        openVenturaSettings: @escaping VenturaOpenAction = {
            if !NSApp.sendAction(Selector(("showSettingsWindow:")), to: nil, from: nil) {
                NSApp.sendAction(Selector(("showPreferencesWindow:")), to: nil, from: nil)
            }
        }
    ) {
        self.activateApp = activateApp
        self.openVenturaSettings = openVenturaSettings
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
            requestRegisteredOpen()
        } else {
            openVenturaSettings()
        }
    }

    func requestRegisteredOpen() {
        guard let action = registrations.last?.action else {
            hasPendingRequest = true
            return
        }
        action()
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
