import SwiftUI
import UniformTypeIdentifiers

struct ManagersSectionView: View {
    private let core = HelmCore.shared
    @ObservedObject private var managersState = HelmCore.shared.managersState
    @EnvironmentObject private var context: ControlCenterContext
    @State private var draggedManagerId: String?

    private var groupedManagers: [(authority: ManagerAuthority, managers: [ManagerInfo])] {
        [
            (authority: .authoritative, managers: managersState.authoritativeManagers),
            (authority: .standard, managers: managersState.standardManagers),
            (authority: .guarded, managers: managersState.guardedManagers)
        ]
    }

    private var hasImplementedManagers: Bool {
        !managersState.authoritativeManagers.isEmpty
            || !managersState.standardManagers.isEmpty
            || !managersState.guardedManagers.isEmpty
    }

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 12) {
                Text(ControlCenterSection.managers.title)
                    .font(.title2.weight(.semibold))
                    .padding(.horizontal, 20)
                    .padding(.top, 20)

                ForEach(groupedManagers, id: \.authority) { group in
                    if !group.managers.isEmpty {
                        VStack(alignment: .leading, spacing: 8) {
                            Text(group.authority.key.localized)
                                .font(.caption.weight(.semibold))
                                .foregroundColor(.secondary)
                                .textCase(.uppercase)
                                .padding(.horizontal, 20)

                            ForEach(group.managers) { manager in
                                ManagerSectionRow(
                                    manager: manager,
                                    status: managersState.managerStatusesById[manager.id],
                                    health: core.health(forManagerId: manager.id),
                                    outdatedCount: managersState.outdatedCountByManager[manager.id, default: 0],
                                    packageCount: managersState.installedCountByManager[manager.id, default: 0],
                                    operationStatus: managersState.managerOperationsById[manager.id],
                                    isManagerUninstalling: core.isManagerUninstalling(manager.id),
                                    isSelected: context.selectedManagerId == manager.id,
                                    onSelect: {
                                        context.selectedManagerId = manager.id
                                        context.selectedPackageId = nil
                                        context.selectedTaskId = nil
                                        context.selectedUpgradePlanStepId = nil
                                    }
                                )
                                .onDrag {
                                    draggedManagerId = manager.id
                                    context.suppressWindowBackgroundDragging = true
                                    return NSItemProvider(object: manager.id as NSString)
                                }
                                .onDrop(
                                    of: [UTType.text.identifier],
                                    delegate: ManagerPriorityDropDelegate(
                                        core: core,
                                        authority: group.authority,
                                        targetManagerId: manager.id,
                                        draggedManagerId: $draggedManagerId,
                                        suppressWindowBackgroundDragging: $context.suppressWindowBackgroundDragging
                                    )
                                )
                            }
                        }
                    }
                }

                if !hasImplementedManagers {
                    Text(L10n.App.ManagersSection.empty.localized)
                        .font(.subheadline)
                        .foregroundColor(.secondary)
                        .padding(.horizontal, 20)
                }
            }
            .padding(.bottom, 18)
        }
        .onHover { hovering in
            context.suppressWindowBackgroundDragging = hovering || draggedManagerId != nil
        }
        .onDisappear {
            draggedManagerId = nil
            context.suppressWindowBackgroundDragging = false
        }
    }
}

private struct ManagerSectionRow: View {
    private let core = HelmCore.shared
    private let installMethodPolicyContext = ManagerInstallMethodPolicyContext.fromEnvironment()

    let manager: ManagerInfo
    let status: ManagerStatus?
    let health: OperationalHealth
    let outdatedCount: Int
    let packageCount: Int
    let operationStatus: String?
    let isManagerUninstalling: Bool
    let isSelected: Bool
    let onSelect: () -> Void

    @State private var showInstallOptionsSheet = false
    @State private var pendingInstallMethodRawValue: String?
    @State private var pendingInstallMethodOptions: [ManagerInstallMethodOption] = []
    @State private var pendingHardTimeoutSeconds: Int?
    @State private var pendingIdleTimeoutSeconds: Int?
    @State private var showAdvancedInstallOptions = false
    @State private var installSubmissionInFlight = false

    private var detected: Bool {
        status?.detected ?? false
    }

    private var enabled: Bool {
        status?.enabled ?? true
    }

    private var isEligibleForEnablement: Bool {
        status?.isEligible ?? true
    }

    private var ineligibleReason: String? {
        guard detected, !isEligibleForEnablement else { return nil }
        if let key = status?.ineligibleServiceErrorKey?.trimmingCharacters(in: .whitespacesAndNewlines),
           !key.isEmpty
        {
            return key.localized
        }
        if let message = status?.ineligibleReasonMessage?.trimmingCharacters(in: .whitespacesAndNewlines),
           !message.isEmpty
        {
            return message
        }
        return nil
    }

    private var enableToggleDisabled: Bool {
        ineligibleReason != nil && !enabled
    }

    private var helmSupportedInstallMethodRawValues: Set<String> {
        switch manager.id {
        case "mise", "mas":
            return ["homebrew"]
        default:
            return Set(manager.installMethodOptions.map(\.method.rawValue))
        }
    }

    private var sortedHelmSupportedInstallMethodOptions: [ManagerInstallMethodOption] {
        manager.installMethodOptions
            .filter { helmSupportedInstallMethodRawValues.contains($0.method.rawValue) }
            .sorted { lhs, rhs in
                let lhsRank = installMethodSortRank(lhs)
                let rhsRank = installMethodSortRank(rhs)
                if lhsRank != rhsRank {
                    return lhsRank < rhsRank
                }
                return localizedInstallMethod(lhs.method)
                    .localizedCaseInsensitiveCompare(localizedInstallMethod(rhs.method)) == .orderedAscending
            }
    }

    private var selectedPendingInstallMethodIsAllowed: Bool {
        guard let pendingInstallMethodRawValue,
              let option = pendingInstallMethodOptions.first(where: {
                  $0.method.rawValue == pendingInstallMethodRawValue
              }) else {
            return false
        }
        return installMethodOptionAllowed(option)
    }

    private var hasAllowedInstallMethodOption: Bool {
        sortedHelmSupportedInstallMethodOptions.contains(where: installMethodOptionAllowed)
    }

    private var hardTimeoutOptions: [Int?] {
        [nil, 120, 300, 600, 900, 1200, 1800]
    }

    private var idleTimeoutOptions: [Int?] {
        [nil, 30, 60, 90, 120, 180, 300, 600]
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 10) {
                Image(systemName: "line.3.horizontal")
                    .font(.caption.weight(.semibold))
                    .foregroundColor(HelmTheme.textSecondary)
                    .frame(width: 12)
                    .accessibilityHidden(true)

                HealthBadgeView(status: health)

                VStack(alignment: .leading, spacing: 3) {
                    Text(localizedManagerDisplayName(manager.id))
                        .font(.body.weight(.medium))
                    HStack(spacing: 6) {
                        Text(L10n.App.Managers.Label.packageCount.localized(with: ["count": packageCount]))
                            .font(.caption)
                            .foregroundColor(.secondary)
                        Text("|")
                            .font(.caption)
                            .foregroundColor(.secondary)
                        Text(L10n.App.Managers.Tooltip.outdated.localized(with: ["count": outdatedCount]))
                            .font(.caption)
                            .foregroundColor(outdatedCount == 0 ? HelmTheme.textSecondary : HelmTheme.stateAttention)
                    }
                }

                Spacer()

                if let operationStatus {
                    HStack(spacing: 4) {
                        ProgressView()
                            .controlSize(.mini)
                        Text(operationStatus)
                            .font(.caption2)
                            .foregroundColor(.secondary)
                    }
                } else {
                    Text(detected ? (enabled ? L10n.App.Managers.State.enabled.localized : L10n.App.Managers.State.disabled.localized) : L10n.App.Managers.State.notInstalled.localized)
                        .font(.caption)
                        .foregroundColor(.secondary)
                }

                if detected {
                    Toggle("", isOn: Binding(
                        get: { enabled },
                        set: { _ in
                            core.setManagerEnabled(manager.id, enabled: !enabled)
                        }
                    ))
                    .toggleStyle(.switch)
                    .labelsHidden()
                    .scaleEffect(0.75)
                    .disabled(enableToggleDisabled || isManagerUninstalling)
                }
            }

            HStack(spacing: 8) {
                if manager.canInstall && !detected {
                    Button(L10n.Common.install.localized) {
                        prepareInstallMethodSelection()
                    }
                    .disabled(installSubmissionInFlight || !hasAllowedInstallMethodOption || isManagerUninstalling)
                    .helmPointer(enabled: !isManagerUninstalling)
                }

                Spacer()

                if enabled && outdatedCount > 0 {
                    Button(L10n.App.Settings.Action.upgradeAll.localized) {
                        core.upgradeAllPackages(forManagerId: manager.id)
                    }
                    .disabled(isManagerUninstalling)
                    .helmPointer(enabled: !isManagerUninstalling)
                }
            }
            .font(.caption)

            if let ineligibleReason {
                Text(ineligibleReason)
                    .font(.caption2)
                    .foregroundColor(.secondary)
            }
        }
        .padding(12)
        .helmCardSurface(cornerRadius: 12)
        .overlay(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .fill(isSelected ? HelmTheme.selectionFill : Color.clear)
                .overlay(
                    RoundedRectangle(cornerRadius: 12, style: .continuous)
                        .strokeBorder(isSelected ? HelmTheme.selectionStroke : Color.clear, lineWidth: 0.9)
                )
                .allowsHitTesting(false)
        )
        .padding(.horizontal, 20)
        .contentShape(Rectangle())
        .onTapGesture {
            onSelect()
        }
        .helmPointer()
        .accessibilityElement(children: .contain)
        .accessibilityLabel(localizedManagerDisplayName(manager.id))
        .accessibilityValue([
            health.key.localized,
            detected ? (enabled ? L10n.App.Managers.State.enabled.localized : L10n.App.Managers.State.disabled.localized) : L10n.App.Managers.State.notInstalled.localized,
            L10n.App.Managers.Label.packageCount.localized(with: ["count": packageCount])
        ].joined(separator: ", "))
        .sheet(isPresented: $showInstallOptionsSheet) {
            VStack(alignment: .leading, spacing: 16) {
                Text(
                    L10n.App.Managers.Alert.installTitle.localized(
                        with: ["manager": localizedManagerDisplayName(manager.id)]
                    )
                )
                .font(.title3.weight(.semibold))

                Text(
                    L10n.App.Managers.Alert.installMessage.localized(
                        with: ["manager_short": manager.shortName]
                    )
                )
                .font(.callout)
                .foregroundColor(.secondary)

                VStack(alignment: .leading, spacing: 8) {
                    Text(L10n.App.Inspector.installMethod.localized)
                        .font(.caption.weight(.semibold))
                        .foregroundColor(.secondary)
                    Picker(
                        L10n.App.Inspector.installMethod.localized,
                        selection: Binding(
                            get: { pendingInstallMethodRawValue ?? "" },
                            set: { pendingInstallMethodRawValue = $0 }
                        )
                    ) {
                        ForEach(pendingInstallMethodOptions) { option in
                            Text(installMethodLabel(option))
                                .tag(option.method.rawValue)
                                .disabled(!installMethodOptionAllowed(option))
                        }
                    }
                    .pickerStyle(.inline)
                }

                DisclosureGroup(
                    L10n.App.Settings.Section.advanced.localized,
                    isExpanded: $showAdvancedInstallOptions
                ) {
                    VStack(alignment: .leading, spacing: 10) {
                        timeoutSelectionRow(
                            label: L10n.App.Inspector.timeoutHard.localized,
                            options: hardTimeoutOptions,
                            selected: pendingHardTimeoutSeconds
                        ) { selection in
                            pendingHardTimeoutSeconds = selection
                        }
                        timeoutSelectionRow(
                            label: L10n.App.Inspector.timeoutIdle.localized,
                            options: idleTimeoutOptions,
                            selected: pendingIdleTimeoutSeconds
                        ) { selection in
                            pendingIdleTimeoutSeconds = selection
                        }
                    }
                    .padding(.top, 6)
                }

                HStack(spacing: 8) {
                    Spacer()
                    Button(L10n.Common.cancel.localized) {
                        showInstallOptionsSheet = false
                    }
                    .keyboardShortcut(.cancelAction)

                    Button(L10n.Common.install.localized) {
                        submitInstallWithSelectedMethod()
                    }
                    .buttonStyle(HelmPrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
                    .disabled(
                        installSubmissionInFlight
                            || pendingInstallMethodRawValue?.isEmpty != false
                            || !selectedPendingInstallMethodIsAllowed
                    )
                }
            }
            .padding(20)
            .frame(minWidth: 420)
        }
    }

    private func prepareInstallMethodSelection() {
        let supportedOptions = sortedHelmSupportedInstallMethodOptions
        guard !supportedOptions.isEmpty else {
            core.installManager(manager.id)
            return
        }

        pendingInstallMethodOptions = supportedOptions
        pendingHardTimeoutSeconds = status?.timeoutHardSeconds
        pendingIdleTimeoutSeconds = status?.timeoutIdleSeconds
        showAdvancedInstallOptions = false
        let allowedOptions = supportedOptions.filter(installMethodOptionAllowed)
        if let selectedRaw = status?.selectedInstallMethod,
           allowedOptions.contains(where: { $0.method.rawValue == selectedRaw }) {
            pendingInstallMethodRawValue = selectedRaw
        } else {
            pendingInstallMethodRawValue =
                allowedOptions.first(where: \.isRecommended)?.method.rawValue
                ?? allowedOptions.first(where: \.isPreferred)?.method.rawValue
                ?? allowedOptions.first?.method.rawValue
        }
        showInstallOptionsSheet = true
    }

    private func submitInstallWithSelectedMethod() {
        guard let installMethod = pendingInstallMethodRawValue, !installMethod.isEmpty else {
            return
        }
        guard let option = pendingInstallMethodOptions.first(where: { $0.method.rawValue == installMethod }),
              installMethodOptionAllowed(option) else {
            return
        }
        installSubmissionInFlight = true
        core.setManagerInstallMethod(manager.id, installMethod: installMethod) { success in
            guard success else {
                installSubmissionInFlight = false
                return
            }
            core.setManagerTimeoutProfile(
                manager.id,
                hardTimeoutSeconds: pendingHardTimeoutSeconds,
                idleTimeoutSeconds: pendingIdleTimeoutSeconds
            ) { timeoutApplied in
                installSubmissionInFlight = false
                guard timeoutApplied else { return }
                showInstallOptionsSheet = false
                core.installManager(manager.id)
            }
        }
    }

    @ViewBuilder
    private func timeoutSelectionRow(
        label: String,
        options: [Int?],
        selected: Int?,
        onSelect: @escaping (Int?) -> Void
    ) -> some View {
        HStack(spacing: 8) {
            Text(label)
                .font(.caption)
                .foregroundColor(.secondary)
            Spacer(minLength: 8)
            Menu {
                ForEach(options, id: \.self) { seconds in
                    Button {
                        onSelect(seconds)
                    } label: {
                        HStack(spacing: 8) {
                            Text(timeoutMenuLabel(seconds))
                            if seconds == selected {
                                Image(systemName: "checkmark")
                            }
                        }
                    }
                }
            } label: {
                HStack(spacing: 6) {
                    Text(timeoutMenuLabel(selected))
                        .font(.caption.monospacedDigit())
                    Image(systemName: "chevron.down")
                        .font(.caption2)
                        .foregroundColor(.secondary)
                }
            }
            .menuStyle(.borderlessButton)
        }
    }

    private func installMethodSortRank(_ option: ManagerInstallMethodOption) -> Int {
        option.recommendationRank
    }

    private func installMethodOptionAllowed(_ option: ManagerInstallMethodOption) -> Bool {
        option.isAllowed(in: installMethodPolicyContext)
    }

    private func localizedInstallMethod(_ method: ManagerDistributionMethod) -> String {
        switch method {
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

    private func installMethodLabel(_ option: ManagerInstallMethodOption) -> String {
        var value = localizedInstallMethod(option.method)
        var tags: [String] = []
        if option.isRecommended {
            tags.append(L10n.App.Inspector.installMethodTagRecommended.localized)
        } else if option.isPreferred {
            tags.append(L10n.App.Inspector.installMethodTagPreferred.localized)
        }
        switch option.policyTag {
        case .managedRestricted:
            tags.append(L10n.App.Inspector.installMethodTagManagedRestricted.localized)
        case .blockedByPolicy:
            tags.append(L10n.App.Inspector.installMethodTagBlocked.localized)
        case .allowed:
            break
        }
        if !tags.isEmpty {
            value += " (\(tags.joined(separator: ", ")))"
        }
        return value
    }

    private func timeoutMenuLabel(_ seconds: Int?) -> String {
        guard let seconds else {
            return L10n.App.Inspector.timeoutUseDefault.localized
        }
        return L10n.App.Inspector.timeoutSeconds.localized(with: ["seconds": seconds])
    }
}

// Backward compatibility wrapper for legacy references.
struct ManagersView: View {
    @Binding var selectedTab: HelmTab

    var body: some View {
        ManagersSectionView()
    }
}

private struct ManagerPriorityDropDelegate: DropDelegate {
    let core: HelmCore
    let authority: ManagerAuthority
    let targetManagerId: String
    @Binding var draggedManagerId: String?
    @Binding var suppressWindowBackgroundDragging: Bool

    func performDrop(info: DropInfo) -> Bool {
        guard let draggedManagerId else { return false }
        core.moveManagerPriority(
            authority: authority,
            draggedManagerId: draggedManagerId,
            targetManagerId: targetManagerId
        )
        self.draggedManagerId = nil
        suppressWindowBackgroundDragging = true
        return true
    }

    func dropExited(info: DropInfo) {
        if !info.hasItemsConforming(to: [UTType.text.identifier]) {
            draggedManagerId = nil
            suppressWindowBackgroundDragging = true
        }
    }
}
