import SwiftUI

// MARK: - Walkthrough Phase

enum WalkthroughPhase: String, CaseIterable {
    case popover
    case controlCenter
}

// MARK: - Step Definition

struct WalkthroughStepDefinition: Identifiable {
    let id: String
    let phase: WalkthroughPhase
    let index: Int
    let titleKey: String
    let descriptionKey: String
    let targetAnchor: String
    let tooltipEdge: Edge
}

// MARK: - Step Definitions

enum WalkthroughSteps {
    static let popover: [WalkthroughStepDefinition] = [
        WalkthroughStepDefinition(
            id: "popover_1",
            phase: .popover,
            index: 0,
            titleKey: L10n.App.Walkthrough.Popover.Step1.title,
            descriptionKey: L10n.App.Walkthrough.Popover.Step1.description,
            targetAnchor: "healthBadge",
            tooltipEdge: .bottom
        ),
        WalkthroughStepDefinition(
            id: "popover_2",
            phase: .popover,
            index: 1,
            titleKey: L10n.App.Walkthrough.Popover.Step2.title,
            descriptionKey: L10n.App.Walkthrough.Popover.Step2.description,
            targetAnchor: "attentionBanner",
            tooltipEdge: .bottom
        ),
        WalkthroughStepDefinition(
            id: "popover_3",
            phase: .popover,
            index: 2,
            titleKey: L10n.App.Walkthrough.Popover.Step3.title,
            descriptionKey: L10n.App.Walkthrough.Popover.Step3.description,
            targetAnchor: "activeTasks",
            tooltipEdge: .top
        ),
        WalkthroughStepDefinition(
            id: "popover_4",
            phase: .popover,
            index: 3,
            titleKey: L10n.App.Walkthrough.Popover.Step4.title,
            descriptionKey: L10n.App.Walkthrough.Popover.Step4.description,
            targetAnchor: "managerSnapshot",
            tooltipEdge: .top
        ),
        WalkthroughStepDefinition(
            id: "popover_5",
            phase: .popover,
            index: 4,
            titleKey: L10n.App.Walkthrough.Popover.Step5.title,
            descriptionKey: L10n.App.Walkthrough.Popover.Step5.description,
            targetAnchor: "footerActions",
            tooltipEdge: .top
        ),
        WalkthroughStepDefinition(
            id: "popover_6",
            phase: .popover,
            index: 5,
            titleKey: L10n.App.Walkthrough.Popover.Step6.title,
            descriptionKey: L10n.App.Walkthrough.Popover.Step6.description,
            targetAnchor: "searchField",
            tooltipEdge: .bottom
        )
    ]

    static let controlCenter: [WalkthroughStepDefinition] = [
        WalkthroughStepDefinition(
            id: "cc_1",
            phase: .controlCenter,
            index: 0,
            titleKey: L10n.App.Walkthrough.ControlCenter.Step1.title,
            descriptionKey: L10n.App.Walkthrough.ControlCenter.Step1.description,
            targetAnchor: "ccSidebar",
            tooltipEdge: .trailing
        ),
        WalkthroughStepDefinition(
            id: "cc_2",
            phase: .controlCenter,
            index: 1,
            titleKey: L10n.App.Walkthrough.ControlCenter.Step2.title,
            descriptionKey: L10n.App.Walkthrough.ControlCenter.Step2.description,
            targetAnchor: "ccOverview",
            tooltipEdge: .leading
        ),
        WalkthroughStepDefinition(
            id: "cc_3",
            phase: .controlCenter,
            index: 2,
            titleKey: L10n.App.Walkthrough.ControlCenter.Step3.title,
            descriptionKey: L10n.App.Walkthrough.ControlCenter.Step3.description,
            targetAnchor: "ccPackages",
            tooltipEdge: .leading
        ),
        WalkthroughStepDefinition(
            id: "cc_4",
            phase: .controlCenter,
            index: 3,
            titleKey: L10n.App.Walkthrough.ControlCenter.Step4.title,
            descriptionKey: L10n.App.Walkthrough.ControlCenter.Step4.description,
            targetAnchor: "ccTasks",
            tooltipEdge: .leading
        ),
        WalkthroughStepDefinition(
            id: "cc_5",
            phase: .controlCenter,
            index: 4,
            titleKey: L10n.App.Walkthrough.ControlCenter.Step5.title,
            descriptionKey: L10n.App.Walkthrough.ControlCenter.Step5.description,
            targetAnchor: "ccManagers",
            tooltipEdge: .leading
        ),
        WalkthroughStepDefinition(
            id: "cc_6",
            phase: .controlCenter,
            index: 5,
            titleKey: L10n.App.Walkthrough.ControlCenter.Step6.title,
            descriptionKey: L10n.App.Walkthrough.ControlCenter.Step6.description,
            targetAnchor: "ccSettings",
            tooltipEdge: .leading
        ),
        WalkthroughStepDefinition(
            id: "cc_7",
            phase: .controlCenter,
            index: 6,
            titleKey: L10n.App.Walkthrough.ControlCenter.Step7.title,
            descriptionKey: L10n.App.Walkthrough.ControlCenter.Step7.description,
            targetAnchor: "ccUpdates",
            tooltipEdge: .leading
        )
    ]
}

// MARK: - Walkthrough Manager

final class WalkthroughManager: ObservableObject {
    static let shared = WalkthroughManager()

    private static let popoverCompletedKey = "hasCompletedPopoverWalkthrough"
    private static let ccCompletedKey = "hasCompletedControlCenterWalkthrough"

    @Published var activePhase: WalkthroughPhase?
    @Published var currentStepIndex: Int = 0

    var isPopoverWalkthroughActive: Bool {
        activePhase == .popover
    }

    var isControlCenterWalkthroughActive: Bool {
        activePhase == .controlCenter
    }

    var currentStep: WalkthroughStepDefinition? {
        guard let phase = activePhase else { return nil }
        let steps = stepsForPhase(phase)
        guard currentStepIndex < steps.count else { return nil }
        return steps[currentStepIndex]
    }

    var totalSteps: Int {
        guard let phase = activePhase else { return 0 }
        return stepsForPhase(phase).count
    }

    var hasCompletedPopoverWalkthrough: Bool {
        UserDefaults.standard.bool(forKey: Self.popoverCompletedKey)
    }

    var hasCompletedControlCenterWalkthrough: Bool {
        UserDefaults.standard.bool(forKey: Self.ccCompletedKey)
    }

    private init() {}

    func startPopoverWalkthrough() {
        activePhase = .popover
        currentStepIndex = 0
    }

    func startControlCenterWalkthrough() {
        activePhase = .controlCenter
        currentStepIndex = 0
    }

    func advanceStep() {
        guard let phase = activePhase else { return }
        let steps = stepsForPhase(phase)
        if currentStepIndex + 1 < steps.count {
            currentStepIndex += 1
        } else {
            completeCurrentPhase()
        }
    }

    func skip() {
        completeCurrentPhase()
    }

    func resetWalkthroughs() {
        UserDefaults.standard.set(false, forKey: Self.popoverCompletedKey)
        UserDefaults.standard.set(false, forKey: Self.ccCompletedKey)
        activePhase = nil
        currentStepIndex = 0
    }

    private func completeCurrentPhase() {
        guard let phase = activePhase else { return }
        switch phase {
        case .popover:
            UserDefaults.standard.set(true, forKey: Self.popoverCompletedKey)
        case .controlCenter:
            UserDefaults.standard.set(true, forKey: Self.ccCompletedKey)
        }
        activePhase = nil
        currentStepIndex = 0
    }

    private func stepsForPhase(_ phase: WalkthroughPhase) -> [WalkthroughStepDefinition] {
        switch phase {
        case .popover:
            return WalkthroughSteps.popover
        case .controlCenter:
            return WalkthroughSteps.controlCenter
        }
    }
}
