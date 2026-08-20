import SwiftUI
import AppKit

struct TasksSectionView: View {
    @ObservedObject private var core = HelmCore.shared
    @ObservedObject private var localization = LocalizationManager.shared
    @EnvironmentObject private var context: ControlCenterContext
    private let researchActivityProjection = WholeWorkflowResearchDatasetProvider
        .activeActivityProjection()
    @State private var expandedTaskId: String?

    var body: some View {
        Group {
            if let researchActivityProjection {
                researchBody(researchActivityProjection)
            } else {
                productionBody
            }
        }
    }

    private var productionBody: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text(ControlCenterSection.tasks.title)
                    .font(.title2.weight(.semibold))
                Spacer()
                if core.isRefreshing {
                    ProgressView()
                        .controlSize(.small)
                }
            }

            if core.activeTasks.isEmpty {
                Text(L10n.App.TasksSection.empty.localized)
                    .font(.subheadline)
                    .foregroundColor(.secondary)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            } else {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        ForEach(Array(core.activeTasks.indices), id: \.self) { index in
                            let task = core.activeTasks[index]
                            TaskRowView(
                                task: task,
                                onCancel: task.isRunning ? { core.cancelTask(task) } : nil,
                                onDismiss: task.status.lowercased() == "failed" ? { core.dismissTask(task) } : nil,
                                canExpandDetails: task.supportsInlineDetails,
                                isExpanded: expandedTaskId == task.id,
                                isSelected: context.selectedTaskId == task.id,
                                onToggleDetails: {
                                    if expandedTaskId == task.id {
                                        expandedTaskId = nil
                                    } else {
                                        expandedTaskId = task.id
                                    }
                                },
                                onSelect: {
                                    context.selectedTaskId = task.id
                                    context.selectedPackageId = nil
                                    context.selectedUpgradePlanStepId = nil
                                    if let managerId = task.managerId {
                                        context.selectedManagerId = managerId
                                    }
                                    if !task.supportsInlineDetails {
                                        expandedTaskId = nil
                                    }
                                }
                            )
                            if index < core.activeTasks.count - 1 {
                                Divider()
                            }
                        }
                    }
                    .padding(.vertical, 4)
                }
                .helmCardSurface(cornerRadius: 12)
            }
        }
        .padding(20)
        .onChange(of: core.activeTasks.map { "\($0.id):\($0.status)" }) { _ in
            collapseExpandedTaskIfNeeded()
        }
        .onChange(of: context.selectedTaskId) { selectedTaskId in
            if expandedTaskId != selectedTaskId {
                expandedTaskId = nil
            }
        }
    }

    private func researchBody(
        _ projection: WholeWorkflowResearchActivityProjection
    ) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            VStack(alignment: .leading, spacing: 3) {
                Text(ControlCenterSection.tasks.title)
                    .font(.title2.weight(.semibold))
                Text(L10n.App.Activity.Research.subtitle.localized)
                    .font(.callout)
                    .foregroundColor(HelmTheme.textSecondary)
            }

            ScrollView {
                LazyVStack(alignment: .leading, spacing: 10) {
                    ForEach(projection.activities) { activity in
                        ResearchActivityRow(
                            activity: activity,
                            isSelected: context.selectedTaskId == activity.selectionID,
                            onSelect: {
                                context.selectedTaskId = activity.selectionID
                                context.selectedPackageId = nil
                                context.selectedUpgradePlanStepId = nil
                                context.selectedManagerId = activity.managerID
                            }
                        )
                    }
                }
                .padding(.vertical, 2)
            }
        }
        .padding(20)
        .onAppear {
            guard let selectedTaskID = context.selectedTaskId,
                  projection.activity(withSelectionID: selectedTaskID) == nil else {
                return
            }
            context.selectedTaskId = nil
        }
    }

    private func collapseExpandedTaskIfNeeded() {
        guard let expandedTaskId else { return }
        let stillVisible = core.activeTasks.contains {
            $0.id == expandedTaskId && $0.supportsInlineDetails
        }
        if !stillVisible {
            self.expandedTaskId = nil
        }
    }
}

private struct ResearchActivityRow: View {
    let activity: WholeWorkflowResearchActivity
    let isSelected: Bool
    let onSelect: () -> Void

    var body: some View {
        Button(action: onSelect) {
            HStack(alignment: .top, spacing: 12) {
                Image(systemName: activity.statusIcon)
                    .font(.title3.weight(.semibold))
                    .foregroundColor(activity.statusColor)
                    .frame(width: 24, height: 24)
                    .accessibilityHidden(true)

                VStack(alignment: .leading, spacing: 6) {
                    Text(activity.localizedTitle)
                        .font(.headline)
                        .foregroundColor(HelmTheme.textPrimary)

                    Text(activity.localizedSummary)
                        .font(.callout)
                        .foregroundColor(HelmTheme.textSecondary)
                        .fixedSize(horizontal: false, vertical: true)

                    Text(localizedManagerDisplayName(activity.managerID))
                        .font(.caption)
                        .foregroundColor(HelmTheme.textSecondary)
                }

                Spacer(minLength: 12)

                Text(activity.localizedStatus)
                    .font(.caption.weight(.semibold))
                    .foregroundColor(activity.statusColor)
                    .multilineTextAlignment(.trailing)

                Image(systemName: "chevron.right")
                    .font(.caption.weight(.semibold))
                    .foregroundColor(HelmTheme.textSecondary)
                    .accessibilityHidden(true)
            }
            .padding(14)
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .helmCardSurface(cornerRadius: 12, highlighted: isSelected)
        .helmPointer()
        .accessibilityLabel(activity.localizedTitle)
        .accessibilityValue(
            "\(activity.localizedStatus). \(activity.localizedSummary)"
        )
        .accessibilityAddTraits(isSelected ? .isSelected : [])
    }
}

struct ResearchActivityInspectorView: View {
    let activity: WholeWorkflowResearchActivity
    let projection: WholeWorkflowResearchActivityProjection
    @State private var reviewedAction: WholeWorkflowResearchRecoveryAction?
    @State private var copiedDiagnostics = false

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(activity.localizedTitle)
                .font(.title3.weight(.semibold))

            Label(activity.localizedStatus, systemImage: activity.statusIcon)
                .font(.callout.weight(.semibold))
                .foregroundColor(activity.statusColor)

            Text(activity.localizedSummary)
                .font(.callout)
                .foregroundColor(HelmTheme.textSecondary)

            ResearchInspectorField(label: L10n.App.Inspector.taskManager.localized) {
                Text(localizedManagerDisplayName(activity.managerID))
                    .font(.callout)
            }

            ResearchInspectorField(label: L10n.App.Inspector.taskId.localized) {
                Text(activity.selectionID)
                    .font(.caption.monospacedDigit())
            }

            Divider()

            Text(L10n.App.Activity.Research.whatChanged.localized)
                .font(.headline)

            ResearchInspectorField(label: L10n.App.Activity.Research.before.localized) {
                Text(activity.beforeKey.localized)
                    .font(.callout)
                    .fixedSize(horizontal: false, vertical: true)
            }

            ResearchInspectorField(label: L10n.App.Activity.Research.after.localized) {
                Text(activity.afterKey.localized)
                    .font(.callout)
                    .fixedSize(horizontal: false, vertical: true)
            }

            ResearchInspectorField(label: L10n.App.Activity.Research.applyResult.localized) {
                Text(activity.localizedApplyResult)
                    .font(.callout)
            }

            ResearchInspectorField(label: L10n.App.Activity.Research.verificationResult.localized) {
                Text(activity.localizedVerificationResult)
                    .font(.callout)
                    .foregroundColor(
                        activity.verificationResult == .failed
                            ? HelmTheme.stateError
                            : HelmTheme.textSecondary
                    )
            }

            ResearchInspectorField(label: L10n.App.Activity.Research.rollback.localized) {
                Text(activity.recoveryLimitsKey.localized)
                    .font(.callout)
                    .foregroundColor(HelmTheme.stateNeedsReview)
                    .fixedSize(horizontal: false, vertical: true)
            }

            let actions = projection.recoveryActions(for: activity)
            if !actions.isEmpty {
                Divider()
                Text(L10n.App.Activity.Research.recoveryOptions.localized)
                    .font(.headline)

                ForEach(actions) { action in
                    recoveryActionRow(action)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .sheet(item: $reviewedAction) { action in
            ResearchRecoveryReviewSheet(
                action: action,
                dismiss: { reviewedAction = nil }
            )
        }
    }

    private func recoveryActionRow(
        _ action: WholeWorkflowResearchRecoveryAction
    ) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            if action.kind == .retryVerification {
                Button(action.localizedTitle) {
                    handle(action)
                }
                .buttonStyle(HelmPrimaryButtonStyle())
                .disabled(!action.allowed)
                .helmPointer(enabled: action.allowed)
                .accessibilityValue(action.reasonKey.localized)
            } else {
                Button(action.localizedTitle) {
                    handle(action)
                }
                .buttonStyle(HelmSecondaryButtonStyle())
                .disabled(!action.allowed)
                .helmPointer(enabled: action.allowed)
                .accessibilityValue(action.reasonKey.localized)
            }

            if ResearchRecoveryInteractionPolicy.interaction(
                for: action
            ) == .unavailableExplanation {
                Button(unavailableExplanationTitle(for: action)) {
                    handle(action)
                }
                .buttonStyle(HelmTertiaryButtonStyle())
                .helmPointer()
                .accessibilityValue(action.reasonKey.localized)
            }

            Text(action.reasonKey.localized)
                .font(.caption)
                .foregroundColor(HelmTheme.textSecondary)
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityHidden(true)

            if action.kind == .copyDiagnostics, copiedDiagnostics {
                Label(
                    L10n.App.Activity.Research.diagnosticsCopied.localized,
                    systemImage: "checkmark.circle.fill"
                )
                .font(.caption.weight(.medium))
                .foregroundColor(HelmTheme.stateHealthy)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func handle(_ action: WholeWorkflowResearchRecoveryAction) {
        switch ResearchRecoveryInteractionPolicy.interaction(for: action) {
        case .copyDiagnostics:
            let pasteboard = NSPasteboard.general
            pasteboard.clearContents()
            copiedDiagnostics = pasteboard.setString(
                projection.redactedDiagnostics(for: activity),
                forType: .string
            )
            if copiedDiagnostics {
                HelmCore.shared.postAccessibilityAnnouncement(
                    L10n.App.Activity.Research.diagnosticsCopied.localized
                )
            }
        case .readOnlyReview, .unavailableExplanation:
            reviewedAction = action
        }
    }

    private func unavailableExplanationTitle(
        for action: WholeWorkflowResearchRecoveryAction
    ) -> String {
        L10n.App.Activity.Research.explainUnavailable.localized(with: [
            "action": action.localizedTitle,
        ])
    }
}

private struct ResearchInspectorField<Content: View>: View {
    let label: String
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label)
                .font(.caption.weight(.semibold))
                .foregroundColor(HelmTheme.textSecondary)
            content
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
    }
}

private struct ResearchRecoveryReviewSheet: View {
    let action: WholeWorkflowResearchRecoveryAction
    let dismiss: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text(title)
                .font(.title2.weight(.semibold))

            Text(action.reasonKey.localized)
                .font(.body)

            Label(
                L10n.App.Activity.Research.readOnlyNotice.localized,
                systemImage: "lock.shield"
            )
            .font(.callout)
            .foregroundColor(HelmTheme.textSecondary)
            .fixedSize(horizontal: false, vertical: true)

            HStack {
                Spacer()
                Button(L10n.Common.done.localized, action: dismiss)
                    .buttonStyle(HelmPrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
                    .helmPointer()
            }
        }
        .padding(24)
        .frame(width: 420)
    }

    private var title: String {
        switch ResearchRecoveryInteractionPolicy.interaction(for: action) {
        case .unavailableExplanation:
            return L10n.App.Activity.Research.explainUnavailable.localized(with: [
                "action": action.localizedTitle,
            ])
        case .readOnlyReview, .copyDiagnostics:
            return L10n.App.Activity.Research.reviewTitle.localized(with: [
                "action": action.localizedTitle,
            ])
        }
    }
}

extension WholeWorkflowResearchActivity {
    var localizedTitle: String {
        L10n.App.Activity.Research.title.localized(with: [
            "package": packageName,
            "manager": localizedManagerDisplayName(managerID),
        ])
    }

    var localizedStatus: String {
        switch state {
        case .failedVerification:
            return L10n.App.Activity.Research.verificationFailed.localized
        case .deferred:
            return L10n.App.Activity.Research.sourceNotStarted.localized
        }
    }

    var localizedSummary: String {
        switch state {
        case .failedVerification:
            return L10n.App.Activity.Research.appliedUnverified.localized
        case .deferred:
            return L10n.App.Activity.Research.sourceUnchanged.localized
        }
    }

    var localizedApplyResult: String {
        switch applyResult {
        case .applied:
            return L10n.App.Activity.Research.applied.localized
        case .notStarted:
            return L10n.App.Activity.Research.notStarted.localized
        }
    }

    var localizedVerificationResult: String {
        switch verificationResult {
        case .failed:
            return L10n.App.Activity.Research.failed.localized
        case .notRun:
            return L10n.App.Activity.Research.notRun.localized
        }
    }

    var statusIcon: String {
        switch state {
        case .failedVerification:
            return "exclamationmark.triangle.fill"
        case .deferred:
            return "pause.circle.fill"
        }
    }

    var statusColor: Color {
        switch state {
        case .failedVerification:
            return HelmTheme.stateError
        case .deferred:
            return HelmTheme.stateNeedsReview
        }
    }
}

extension WholeWorkflowResearchRecoveryAction {
    var localizedTitle: String {
        switch kind {
        case .retryVerification:
            return L10n.App.Activity.Research.retryVerification.localized
        case .restore:
            return L10n.App.Activity.Research.restore.localized
        case .keep:
            return L10n.App.Activity.Research.keep.localized
        case .copyDiagnostics:
            return L10n.App.Activity.Research.copyDiagnostics.localized
        }
    }
}

// Backward compatibility wrapper for legacy references.
struct TaskListView: View {
    var maxTasks: Int = 10

    var body: some View {
        TasksSectionView()
    }
}
