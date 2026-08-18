import Foundation

struct ControlCenterGlobalSearchNavigationDecision {
    let deepLink: WayfinderDeepLink
    let managerFilterID: String?
}

enum ControlCenterGlobalSearchNavigationPolicy {
    static func acceptedResultNavigation(
        packageID: String
    ) -> ControlCenterGlobalSearchNavigationDecision? {
        guard let deepLink = acceptedResultDeepLink(packageID: packageID) else { return nil }
        return ControlCenterGlobalSearchNavigationDecision(
            deepLink: deepLink,
            managerFilterID: nil
        )
    }

    static func acceptedResultDeepLink(packageID: String) -> WayfinderDeepLink? {
        let packageID = packageID.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !packageID.isEmpty else { return nil }
        return WayfinderDeepLink(
            destination: .library,
            entityID: packageID,
            focus: .selectedEntity
        )
    }
}

protocol ControlCenterSearchFocusTarget: AnyObject {
    func requestSearchFocus(completion: @escaping () -> Void)
}

final class ControlCenterSearchFocusRouter {
    private weak var target: ControlCenterSearchFocusTarget?
    private var nextRequestID = 0
    private var pendingRequestID: Int?

    func attach(_ target: ControlCenterSearchFocusTarget) {
        self.target = target
        deliverPendingRequest()
    }

    func detach(_ target: ControlCenterSearchFocusTarget) {
        guard self.target === target else { return }
        self.target = nil
    }

    func requestFocus() {
        nextRequestID += 1
        pendingRequestID = nextRequestID
        deliverPendingRequest()
    }

    private func deliverPendingRequest() {
        guard let requestID = pendingRequestID, let target else { return }
        target.requestSearchFocus { [weak self] in
            guard self?.pendingRequestID == requestID else { return }
            self?.pendingRequestID = nil
        }
    }
}

final class ControlCenterSearchTextUpdateGate {
    private(set) var isApplyingModelValue = false
    private(set) var hasScheduledControlPublish = false
    private var pendingControlValue: String?

    func applyModelValue(_ update: () -> Void) {
        isApplyingModelValue = true
        defer { isApplyingModelValue = false }
        update()
    }

    func shouldPublishControlValue(_ controlValue: String, modelValue: String) -> Bool {
        !isApplyingModelValue && controlValue != modelValue
    }

    func stageControlValue(_ controlValue: String, modelValue: String) -> Bool {
        guard shouldPublishControlValue(controlValue, modelValue: modelValue) else { return false }
        pendingControlValue = controlValue
        guard !hasScheduledControlPublish else { return false }
        hasScheduledControlPublish = true
        return true
    }

    func displayedValue(modelValue: String) -> String {
        pendingControlValue ?? modelValue
    }

    func takePendingControlValue() -> String? {
        hasScheduledControlPublish = false
        defer { pendingControlValue = nil }
        return pendingControlValue
    }
}
