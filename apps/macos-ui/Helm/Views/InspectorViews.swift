import SwiftUI

struct ControlCenterInspectorView: View {
    @ObservedObject private var core = HelmCore.shared
    @EnvironmentObject private var context: ControlCenterContext

    private var selectedTask: TaskItem? {
        guard let taskId = context.selectedTaskId else { return nil }
        return core.activeTasks.first { $0.id == taskId }
    }

    private var selectedPackage: PackageItem? {
        guard let packageId = context.selectedPackageId else { return nil }
        return core.allKnownPackages.first { $0.id == packageId }
    }

    private var selectedUpgradePlanTask: TaskItem? {
        guard let stepId = context.selectedUpgradePlanStepId,
              let step = core.upgradePlanSteps.first(where: { $0.id == stepId }) else { return nil }
        return TaskItem(
            id: step.id,
            description: core.localizedUpgradePlanReason(for: step),
            status: step.status,
            managerId: step.managerId,
            taskType: step.action,
            labelKey: step.reasonLabelKey,
            labelArgs: step.reasonLabelArgs
        )
    }

    private var selectedManager: ManagerInfo? {
        guard let managerId = context.selectedManagerId else { return nil }
        return ManagerInfo.all.first { $0.id == managerId }
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                Text(L10n.App.Inspector.title.localized)
                    .font(.headline)

                if let task = selectedTask ?? selectedUpgradePlanTask {
                    InspectorTaskDetailView(task: task)
                } else if let package = selectedPackage {
                    InspectorPackageDetailView(package: package)
                } else if let manager = selectedManager {
                    InspectorManagerDetailView(
                        manager: manager,
                        status: core.managerStatuses[manager.id],
                        health: core.health(forManagerId: manager.id),
                        packageCount: core.installedPackages.filter { $0.managerId == manager.id }.count,
                        outdatedCount: core.outdatedCount(forManagerId: manager.id),
                        onViewPackages: {
                            context.managerFilterId = manager.id
                            context.selectedUpgradePlanStepId = nil
                            context.selectedSection = .packages
                        }
                    )
                } else {
                    Text(L10n.App.Inspector.empty.localized)
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }

                Spacer()
            }
            .padding(14)
        }
    }
}

// MARK: - Task Inspector

private struct InspectorTaskDetailView: View {
    let task: TaskItem

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(task.description)
                .font(.title3.weight(.semibold))

            // Status badge
            HStack(spacing: 6) {
                Image(systemName: task.statusIcon)
                    .foregroundStyle(task.statusColor)
                Text(task.localizedStatus)
                    .font(.callout.weight(.medium))
                    .foregroundStyle(task.statusColor)
            }
            .accessibilityElement(children: .combine)
            .accessibilityLabel(L10n.App.Inspector.taskStatus.localized)
            .accessibilityValue(task.localizedStatus)

            InspectorField(label: L10n.App.Inspector.taskId.localized) {
                Text(task.id)
                    .font(.caption.monospaced())
                    .accessibilityLabel(L10n.App.Inspector.taskId.localized)
                    .accessibilityValue(task.id)
            }

            if let taskType = task.taskType {
                InspectorField(label: L10n.App.Inspector.taskType.localized) {
                    Text(localizedTaskType(taskType))
                        .font(.callout)
                }
            }

            if let managerId = task.managerId {
                InspectorField(label: L10n.App.Inspector.taskManager.localized) {
                    Text(localizedManagerDisplayName(managerId))
                        .font(.callout)
                }
            }

            if let labelKey = task.labelKey {
                InspectorField(label: L10n.App.Inspector.taskLabelKey.localized) {
                    Text(labelKey)
                        .font(.caption.monospaced())
                        .foregroundStyle(.secondary)
                        .accessibilityLabel(L10n.App.Inspector.taskLabelKey.localized)
                        .accessibilityValue(labelKey)
                }
            }

            if let labelArgs = task.labelArgs, !labelArgs.isEmpty {
                InspectorField(label: L10n.App.Inspector.taskLabelArgs.localized) {
                    VStack(alignment: .leading, spacing: 2) {
                        ForEach(labelArgs.sorted(by: { $0.key < $1.key }), id: \.key) { key, value in
                            HStack(spacing: 4) {
                                Text(key)
                                    .font(.caption.monospaced())
                                    .foregroundStyle(.secondary)
                                Text(":")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                                Text(value)
                                    .font(.caption.monospaced())
                            }
                        }
                    }
                }
            }

            if task.status.lowercased() == "failed" {
                InspectorField(label: L10n.App.Inspector.taskFailureFeedback.localized) {
                    Text(failureHintText())
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        }
    }

    private func localizedTaskType(_ rawType: String) -> String {
        HelmCore.shared.localizedTaskType(rawType)
    }

    private func failureHintText() -> String {
        if task.labelKey == "service.task.label.install.homebrew_formula",
           let package = task.labelArgs?["package"] {
            return L10n.App.Inspector.taskFailureHintHomebrewInstall.localized(with: [
                "package": package
            ])
        }

        if let managerId = task.managerId {
            return L10n.App.Inspector.taskFailureHintGeneric.localized(with: [
                "manager": localizedManagerDisplayName(managerId)
            ])
        }

        return L10n.Service.Error.processFailure.localized
    }
}

// MARK: - Package Inspector

private struct InspectorPackageDetailView: View {
    @ObservedObject private var core = HelmCore.shared
    @EnvironmentObject private var context: ControlCenterContext
    let package: PackageItem

    private var packageDescriptionText: String? {
        guard let summary = package.summary?.trimmingCharacters(in: .whitespacesAndNewlines),
              !summary.isEmpty else {
            return nil
        }
        return summary
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(package.name)
                .font(.title3.weight(.semibold))

            InspectorField(label: L10n.App.Inspector.manager.localized) {
                Text(localizedManagerDisplayName(package.managerId))
                    .font(.callout)
            }

            // Status badge
            HStack(spacing: 6) {
                Image(systemName: package.status.iconName)
                    .foregroundStyle(package.status.iconColor)
                Text(package.status.displayName)
                    .font(.callout.weight(.medium))
                    .foregroundStyle(package.status.iconColor)
            }
            .accessibilityElement(children: .combine)
            .accessibilityLabel(L10n.App.Inspector.packageStatus.localized)
            .accessibilityValue(package.status.displayName)

            InspectorField(label: L10n.App.Inspector.installed.localized) {
                Text(package.version)
                    .font(.caption.monospaced())
            }

            if let latest = package.latestVersion {
                InspectorField(label: L10n.App.Inspector.latest.localized) {
                    Text(latest)
                        .font(.caption.monospaced())
                }
            }

            if package.pinned {
                HStack(spacing: 6) {
                    Image(systemName: "pin.fill")
                        .foregroundStyle(.orange)
                        .font(.caption)
                    Text(L10n.App.Inspector.pinned.localized)
                        .font(.callout)
                }
                .accessibilityElement(children: .combine)
                .accessibilityLabel(L10n.App.Inspector.pinned.localized)
            }

            if package.restartRequired {
                HStack(spacing: 6) {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundStyle(.orange)
                        .font(.caption)
                    Text(L10n.App.Inspector.restartRequired.localized)
                        .font(.callout)
                }
                .accessibilityElement(children: .combine)
                .accessibilityLabel(L10n.App.Inspector.restartRequired.localized)
            }

            InspectorField(label: L10n.App.Inspector.description.localized) {
                if let packageDescriptionText {
                    Text(packageDescriptionText)
                        .font(.caption)
                } else if core.packageDescriptionLoadingIds.contains(package.id) {
                    Text(L10n.App.Inspector.descriptionLoading.localized)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                } else if core.packageDescriptionUnavailableIds.contains(package.id) {
                    Text(L10n.App.Inspector.descriptionUnavailable.localized)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                } else {
                    Text(L10n.App.Inspector.descriptionLoading.localized)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }

            HStack(spacing: 8) {
                if core.canInstallPackage(package) {
                    Button(L10n.App.Packages.Action.install.localized) {
                        core.installPackage(package)
                    }
                    .buttonStyle(HelmSecondaryButtonStyle())
                    .disabled(core.installActionPackageIds.contains(package.id))
                    .helmPointer(enabled: !core.installActionPackageIds.contains(package.id))
                }

                if core.canUninstallPackage(package) {
                    Button(L10n.App.Packages.Action.uninstall.localized) {
                        core.uninstallPackage(package)
                    }
                    .buttonStyle(HelmSecondaryButtonStyle())
                    .disabled(core.uninstallActionPackageIds.contains(package.id))
                    .helmPointer(enabled: !core.uninstallActionPackageIds.contains(package.id))
                }

                if core.canUpgradeIndividually(package) {
                    Button(L10n.Common.update.localized) {
                        core.upgradePackage(package)
                    }
                    .buttonStyle(HelmSecondaryButtonStyle())
                    .disabled(core.upgradeActionPackageIds.contains(package.id))
                    .helmPointer(enabled: !core.upgradeActionPackageIds.contains(package.id))
                }

                if core.canPinPackage(package) {
                    Button(package.pinned ? L10n.App.Packages.Action.unpin.localized : L10n.App.Packages.Action.pin.localized) {
                        if package.pinned {
                            core.unpinPackage(package)
                        } else {
                            core.pinPackage(package)
                        }
                    }
                    .buttonStyle(HelmSecondaryButtonStyle())
                    .disabled(core.pinActionPackageIds.contains(package.id))
                    .helmPointer(enabled: !core.pinActionPackageIds.contains(package.id))
                }

                Button(L10n.App.Inspector.viewManager.localized) {
                    context.selectedManagerId = package.managerId
                    context.selectedPackageId = nil
                    context.selectedTaskId = nil
                    context.selectedUpgradePlanStepId = nil
                    context.selectedSection = .managers
                }
                .buttonStyle(HelmSecondaryButtonStyle())
                .helmPointer()
            }
            .font(.caption)

            InspectorField(label: L10n.App.Inspector.packageId.localized) {
                Text(package.id)
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
                    .accessibilityLabel(L10n.App.Inspector.packageId.localized)
                    .accessibilityValue(package.id)
            }
        }
        .onAppear {
            core.ensurePackageDescription(for: package)
        }
        .onChange(of: package.id) { _ in
            core.ensurePackageDescription(for: package)
        }
        .onChange(of: package.summary) { _ in
            core.ensurePackageDescription(for: package)
        }
    }
}

// MARK: - Manager Inspector

private struct InspectorManagerDetailView: View {
    let manager: ManagerInfo
    let status: ManagerStatus?
    let health: OperationalHealth
    let packageCount: Int
    let outdatedCount: Int
    let onViewPackages: () -> Void

    private var detected: Bool {
        status?.detected ?? false
    }

    private var enabled: Bool {
        status?.enabled ?? true
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 8) {
                Image(systemName: manager.symbolName)
                    .foregroundStyle(.secondary)
                Text(localizedManagerDisplayName(manager.id))
                    .font(.title3.weight(.semibold))
                Spacer()
                HealthBadgeView(status: health)
            }

            HStack(spacing: 6) {
                Text(L10n.App.Managers.Label.packageCount.localized(with: ["count": packageCount]))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Text("|")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Text(L10n.App.Managers.Tooltip.outdated.localized(with: ["count": outdatedCount]))
                    .font(.caption)
                    .foregroundStyle(outdatedCount == 0 ? Color.secondary : Color.orange)
            }

            InspectorField(label: L10n.App.Inspector.category.localized) {
                Text(localizedCategoryName(manager.category))
                    .font(.callout)
            }

            Text(manager.authority.key.localized)
                .font(.callout)
                .foregroundStyle(.secondary)

            Group {
                // Detection status
                HStack(spacing: 6) {
                    Image(systemName: detected ? "checkmark.circle.fill" : "xmark.circle")
                        .foregroundStyle(detected ? .green : .red)
                    Text(detected
                        ? L10n.App.Inspector.detected.localized
                        : L10n.App.Inspector.notDetected.localized)
                        .font(.callout)
                }
                .accessibilityElement(children: .combine)
                .accessibilityValue(detected
                    ? L10n.App.Inspector.detected.localized
                    : L10n.App.Inspector.notDetected.localized)

                if let version = status?.version {
                    InspectorField(label: L10n.App.Inspector.version.localized) {
                        Text(version)
                            .font(.caption.monospaced())
                    }
                }

                if let path = status?.executablePath {
                    InspectorField(label: L10n.App.Inspector.executablePath.localized) {
                        Text(path)
                            .font(.caption.monospaced())
                            .lineLimit(2)
                            .accessibilityLabel(L10n.App.Inspector.executablePath.localized)
                            .accessibilityValue(path)
                    }
                }

                // Enabled/Disabled
                HStack(spacing: 6) {
                    Image(systemName: enabled ? "checkmark.circle.fill" : "minus.circle.fill")
                        .foregroundStyle(enabled ? .green : .secondary)
                    Text(enabled
                        ? L10n.App.Inspector.enabled.localized
                        : L10n.App.Inspector.disabled.localized)
                        .font(.callout)
                }
                .accessibilityElement(children: .combine)
                .accessibilityValue(enabled
                    ? L10n.App.Inspector.enabled.localized
                    : L10n.App.Inspector.disabled.localized)
            }

            InspectorField(label: L10n.App.Inspector.installMethod.localized) {
                Text(localizedInstallMethod(manager.installMethod))
                    .font(.callout)
            }

            InspectorField(label: L10n.App.Inspector.capabilities.localized) {
                VStack(alignment: .leading, spacing: 2) {
                    ForEach(manager.capabilities, id: \.self) { capabilityKey in
                        Text(capabilityKey.localized)
                            .font(.caption)
                    }
                }
            }

            if packageCount > 0 {
                Button(L10n.App.Managers.Action.viewPackages.localized) {
                    onViewPackages()
                }
                .font(.caption)
                .helmPointer()
            }
        }
    }

    private func localizedCategoryName(_ category: String) -> String {
        switch category {
        case "Toolchain": return L10n.App.Managers.Category.toolchain.localized
        case "System/OS": return L10n.App.Managers.Category.systemOs.localized
        case "Language": return L10n.App.Managers.Category.language.localized
        case "App Store": return L10n.App.Managers.Category.appStore.localized
        default: return category
        }
    }

    private func localizedInstallMethod(_ method: ManagerInstallMethod) -> String {
        switch method {
        case .automatable: return L10n.App.Inspector.InstallMethod.automatable.localized
        case .updateAndUninstall: return L10n.App.Inspector.InstallMethod.updateAndUninstall.localized
        case .updateOnly: return L10n.App.Inspector.InstallMethod.updateOnly.localized
        case .systemBinary: return L10n.App.Inspector.InstallMethod.systemBinary.localized
        case .notManageable: return L10n.App.Inspector.InstallMethod.notManageable.localized
        }
    }
}

// MARK: - Helper

private struct InspectorField<Content: View>: View {
    let label: String
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label)
                .font(.caption)
                .foregroundStyle(.secondary)
            content
        }
        .accessibilityElement(children: .combine)
    }
}
