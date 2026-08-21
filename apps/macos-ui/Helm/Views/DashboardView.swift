import AppKit
import SwiftUI

struct WayfinderPopoverView: View {
    @ObservedObject private var core = HelmCore.shared
    @ObservedObject private var overviewState = HelmCore.shared.overviewState
    @ObservedObject private var appUpdate = AppUpdateCoordinator.shared
    @ObservedObject private var localization = LocalizationManager.shared
    @EnvironmentObject private var context: ControlCenterContext
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Environment(\.colorScheme) private var colorScheme
    @State private var showsQuitConfirmation = false
    private let popoverFixture = WayfinderPopoverFixtureProvider.active()
    private let researchAmbientHealthPresentation = WholeWorkflowResearchDatasetProvider
        .activeAmbientHealthPresentation()

    let onOpenControlCenter: () -> Void
    let onOpenSettings: () -> Void
    let onClosePopover: () -> Void

    private var presentation: WayfinderPopoverPresentation {
        if let popoverFixture {
            return popoverFixture.presentation
        }
        if let researchAmbientHealthPresentation {
            return researchAmbientHealthPresentation
        }
        return WayfinderPopoverPresentationProjector.content(
            for: WayfinderPopoverPresentationInput(
                projection: overviewState.wayfinderProjection.content,
                relatedRouteStages: overviewState.wayfinderRelatedRouteStages,
                relatedManagerIDsByStage: overviewState.wayfinderRelatedManagerIDsByStage,
                detectedManagerCount: overviewState.detectedManagerCount,
                findingContext: overviewState.wayfinderFindingContext
            )
        )
    }

    var body: some View {
        Group {
            if popoverFixture == nil
                && researchAmbientHealthPresentation == nil
                && (!core.hasCompletedOnboarding || core.requiresLicenseTermsAcceptance) {
                OnboardingContainerView {
                    core.completeOnboarding()
                    core.triggerRefresh()
                }
                .frame(
                    width: WayfinderPopoverLayout.width,
                    height: WayfinderPopoverLayout.onboardingHeight
                )
            } else {
                popoverContent
            }
        }
        .alert(
            L10n.App.Overlay.Quit.title.localized,
            isPresented: $showsQuitConfirmation
        ) {
            Button(L10n.Common.cancel.localized, role: .cancel) {}
            Button(L10n.App.Settings.Action.quit.localized, role: .destructive) {
                onClosePopover()
                NSApp.terminate(nil)
            }
        } message: {
            Text(
                L10n.App.Overlay.Quit.message.localized(
                    with: ["tasks": overviewState.runningTaskCount]
                )
            )
        }
    }

    private var popoverContent: some View {
        ZStack(alignment: .topTrailing) {
            VStack(spacing: 0) {
                header
                Divider().overlay(HelmTheme.borderSubtle)
                hero
                routeStrip
                contextRow
                Divider()
                    .overlay(HelmTheme.borderSubtle)
                    .padding(.horizontal, 17)
                commandRows
            }

            utilityMenu
                .padding(.top, 14)
                .padding(.trailing, 17)
        }
        .frame(
            width: WayfinderPopoverLayout.width,
            height: WayfinderPopoverLayout.ordinaryHeight,
            alignment: .top
        )
        .foregroundColor(HelmTheme.textPrimary)
        .background(popoverBackground)
        .accessibilityIdentifier("wayfinderPopover")
    }

    private var header: some View {
        HStack(spacing: 10) {
            Image(nsImage: NSApp.applicationIconImage)
                .resizable()
                .interpolation(.high)
                .aspectRatio(contentMode: .fit)
                .frame(width: 34, height: 34)
                .accessibilityHidden(true)

            VStack(alignment: .leading, spacing: 1) {
                Text(L10n.App.Dashboard.title.localized)
                    .font(.system(size: 13, weight: .semibold))
                Text(freshnessText)
                    .font(.system(size: 10, weight: .medium))
                    .foregroundColor(HelmTheme.textSecondary)
                    .lineLimit(1)
            }
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(
                "\(L10n.App.Dashboard.title.localized). \(freshnessText)"
            )

            Spacer()
        }
        .padding(.horizontal, 17)
        .frame(height: 56)
        .accessibilityElement(children: .contain)
    }

    private var utilityMenu: some View {
        Menu {
            Button(L10n.Common.settings.localized) {
                onOpenSettings()
            }

            Button(L10n.App.Overlay.About.checkForUpdates.localized) {
                onClosePopover()
                appUpdate.checkForUpdates()
            }
            .disabled(!appUpdate.canCheckForUpdates || appUpdate.isCheckingForUpdates)

            Divider()

            Button(L10n.App.Settings.SupportFeedback.section.localized) {
                onClosePopover()
                context.settingsOpenRouter.requestOpen(pane: .support)
            }

            Button(L10n.App.Overlay.About.title.localized) {
                onClosePopover()
                DispatchQueue.main.async {
                    NSApp.orderFrontStandardAboutPanel(nil)
                    NSApp.activate(ignoringOtherApps: true)
                }
            }

            Divider()

            Button(L10n.App.Settings.Action.quit.localized) {
                requestQuit()
            }
        } label: {
            ZStack {
                Circle()
                    .fill(HelmTheme.surfaceElevated.opacity(0.92))
                Circle()
                    .strokeBorder(HelmTheme.borderSubtle, lineWidth: 1)
                Image(systemName: "ellipsis")
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundColor(HelmTheme.textSecondary)
            }
            .frame(width: 28, height: 28)
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
        .help("app.popover.wayfinder.utilities".localized)
        .accessibilityLabel("app.popover.wayfinder.utilities".localized)
        .accessibilityHint("app.popover.wayfinder.utilities.hint".localized)
        .accessibilitySortPriority(-1)
    }

    private var hero: some View {
        HStack(spacing: 17) {
            WayfinderPopoverCourseIndicator(
                projection: presentation.projection
            )
                .frame(width: 92, height: 92)

            VStack(alignment: .leading, spacing: 5) {
                Text(presentation.projection.title.localized)
                    .font(.system(size: 19, weight: .semibold, design: .rounded))
                    .lineLimit(2)
                    .minimumScaleFactor(0.86)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityAddTraits(.isHeader)
                    .accessibilityHidden(presentation.projection.progress != nil)

                Text(presentation.projection.explanation.localized)
                    .font(.system(size: 10.5, weight: .medium))
                    .foregroundColor(HelmTheme.textSecondary)
                    .lineLimit(2)
                    .minimumScaleFactor(0.9)
                    .fixedSize(horizontal: false, vertical: true)

                if presentation.showsPrimaryAction {
                    Button(presentation.primaryActionTitle.localized) {
                        navigate(to: presentation.projection.primaryAction)
                    }
                    .buttonStyle(WayfinderPopoverPrimaryButtonStyle())
                    .padding(.top, 2)
                } else if presentation.projection.condition == .healthy {
                    Label(
                        "app.popover.wayfinder.no_action_needed".localized,
                        systemImage: "checkmark"
                    )
                    .font(.system(size: 10.5, weight: .semibold))
                    .foregroundColor(HelmTheme.stateHealthy)
                    .padding(.top, 3)
                    .accessibilityHidden(true)
                }
            }

            Spacer(minLength: 0)
        }
        .padding(.horizontal, 19)
        .frame(height: 142)
        .background(heroBackground)
    }

    private var routeStrip: some View {
        HStack(spacing: 0) {
            ForEach(Array(presentation.routeItems.enumerated()), id: \.element.stage.rawValue) { index, item in
                routeStage(item)
                if index < presentation.routeItems.count - 1 {
                    Rectangle()
                        .fill(routeConnectorColor(for: item.tone))
                        .frame(maxWidth: .infinity)
                        .frame(height: 1.5)
                        .offset(y: -8)
                        .accessibilityHidden(true)
                }
            }
        }
        .padding(.horizontal, 17)
        .frame(height: 67)
        .background(HelmTheme.surfaceElevated.opacity(0.46))
    }

    private func routeStage(_ item: WayfinderPopoverRouteItem) -> some View {
        Button {
            navigate(
                to: item.deepLink(
                    originatingCondition: presentation.projection.condition.kind
                )
            )
        } label: {
            VStack(spacing: 4) {
                ZStack(alignment: .bottomTrailing) {
                    Circle()
                        .fill(routeColor(for: item.tone).opacity(0.11))
                    Image(systemName: item.stage.symbol)
                        .font(.system(size: 10, weight: .medium))
                        .foregroundColor(routeColor(for: item.tone).opacity(0.9))

                    Image(systemName: routeStatusSymbol(for: item.tone))
                        .font(.system(size: 5.5, weight: .bold))
                        .foregroundColor(.white)
                        .frame(width: 10, height: 10)
                        .background(routeColor(for: item.tone), in: Circle())
                        .offset(x: 2, y: 2)
                }
                .frame(width: 27, height: 27)

                Text(item.stage.titleKey.localized)
                    .font(.system(size: 9.5, weight: .semibold))
                    .foregroundColor(
                        item.tone == .cached
                            ? HelmTheme.textSecondary
                            : HelmTheme.textPrimary.opacity(0.72)
                    )
                    .lineLimit(1)
            }
            .frame(width: 57)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .helmPointer()
        .accessibilityLabel(item.stage.titleKey.localized)
        .accessibilityValue(routeToneTitleKey(for: item.tone).localized)
        .accessibilityHint("app.popover.wayfinder.route.hint".localized)
    }

    private var contextRow: some View {
        HStack(spacing: 11) {
            ZStack {
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(accentColor.opacity(0.12))
                Image(systemName: presentation.contextSymbol)
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundColor(accentColor)
            }
            .frame(width: 31, height: 31)
            .accessibilityHidden(true)

            VStack(alignment: .leading, spacing: 1) {
                Text(presentation.contextTitle.localized)
                    .font(.system(size: 11, weight: .semibold))
                Text(presentation.contextDetail.localized)
                    .font(.system(size: 10, weight: .medium))
                    .foregroundColor(HelmTheme.textSecondary)
                    .lineLimit(1)
                    .minimumScaleFactor(0.82)
            }

            Spacer(minLength: 0)
        }
        .padding(.horizontal, 18)
        .frame(height: 56)
        .accessibilityElement(children: .combine)
    }

    private var commandRows: some View {
        VStack(spacing: 0) {
            commandRow(
                L10n.App.Action.openControlCenter.localized,
                symbol: "rectangle.3.group",
                shortcut: "⌘1"
            ) {
                navigate(
                    to: WayfinderDeepLink(
                        destination: .dashboard,
                        entityID: nil,
                        focus: .primaryContent,
                        originatingCondition: presentation.projection.condition.kind
                    )
                )
            }

            commandRow(
                "app.command.find_software".localized,
                symbol: "magnifyingglass",
                shortcut: "⌘F"
            ) {
                context.select(.packages)
                context.controlCenterSearchFocusRouter.requestFocus()
                onOpenControlCenter()
            }

            commandRow(
                presentation.allowsRefresh
                    ? "app.popover.wayfinder.check_again".localized
                    : "app.popover.wayfinder.check_when_online".localized,
                symbol: presentation.allowsRefresh ? "arrow.clockwise" : "wifi.slash",
                shortcut: presentation.allowsRefresh ? "⌘R" : nil,
                enabled: presentation.allowsRefresh && !core.isRefreshing
            ) {
                core.triggerRefresh()
            }
        }
        .padding(.vertical, 6)
    }

    private func commandRow(
        _ title: String,
        symbol: String,
        shortcut: String?,
        enabled: Bool = true,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            HStack(spacing: 11) {
                Image(systemName: symbol)
                    .font(.system(size: 12, weight: .medium))
                    .foregroundColor(
                        enabled
                            ? HelmTheme.textSecondary
                            : HelmTheme.textSecondary.opacity(0.52)
                    )
                    .frame(width: 18)

                Text(title)
                    .font(.system(size: 12, weight: .medium))
                    .foregroundColor(
                        enabled
                            ? HelmTheme.textPrimary
                            : HelmTheme.textSecondary.opacity(0.62)
                    )

                Spacer()

                if let shortcut {
                    Text(shortcut)
                        .font(.system(size: 10, weight: .medium, design: .rounded))
                        .foregroundColor(HelmTheme.textSecondary)
                }
            }
            .padding(.horizontal, 18)
            .frame(height: 36)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
        .helmPointer(enabled: enabled)
    }

    private var popoverBackground: some View {
        ZStack {
            if reduceTransparency {
                HelmTheme.surfaceBase
            } else {
                HelmTheme.surfacePanel.opacity(colorScheme == .dark ? 0.97 : 0.99)
                LinearGradient(
                    colors: colorScheme == .dark
                        ? [
                            HelmTheme.blue900.opacity(0.34),
                            HelmTheme.surfaceBase.opacity(0.92)
                        ]
                        : [
                            HelmTheme.blue500.opacity(0.08),
                            HelmTheme.surfacePanel.opacity(0.98)
                        ],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                )
            }
        }
    }

    private var heroBackground: some View {
        ZStack {
            LinearGradient(
                colors: [accentColor.opacity(colorScheme == .dark ? 0.12 : 0.08), .clear],
                startPoint: .leading,
                endPoint: .trailing
            )
            WayfinderPopoverHorizonLines()
                .stroke(
                    accentColor.opacity(colorScheme == .dark ? 0.09 : 0.11),
                    lineWidth: 1
                )
        }
        .clipped()
    }

    private var accentColor: Color {
        switch presentation.projection.condition {
        case .healthy:
            return HelmTheme.stateHealthy
        case .updatesReady:
            return HelmTheme.stateUpdatesReady
        case .activeWork, .refreshing:
            return HelmTheme.stateRunning
        case .approvalRequired, .actionableFinding:
            return HelmTheme.stateNeedsReview
        case .failedOrInterrupted:
            return HelmTheme.stateError
        case .offline, .serviceUnavailable:
            return HelmTheme.stateUnavailable
        }
    }

    private var freshnessText: String {
        switch presentation.projection.condition {
        case .activeWork, .refreshing:
            return "app.popover.wayfinder.freshness.working_now".localized
        default:
            break
        }

        guard let date = presentation.projection.freshnessDate else {
            return "app.popover.wayfinder.freshness.not_checked".localized
        }
        let formatter = RelativeDateTimeFormatter()
        formatter.locale = Locale(identifier: localization.currentLocale)
        formatter.unitsStyle = .short
        let relative = formatter.localizedString(for: date, relativeTo: Date())
        let key = presentation.projection.condition == .offline
            ? "app.popover.wayfinder.freshness.saved"
            : "app.popover.wayfinder.freshness.checked"
        return key.localized(with: ["time": relative])
    }

    private func navigate(to deepLink: WayfinderDeepLink) {
        context.navigate(to: deepLink)
        onOpenControlCenter()
    }

    private func requestQuit() {
        if overviewState.runningTaskCount > 0 {
            showsQuitConfirmation = true
        } else {
            onClosePopover()
            NSApp.terminate(nil)
        }
    }

    private func routeColor(for tone: WayfinderPopoverRouteTone) -> Color {
        switch tone {
        case .current:
            return HelmTheme.stateHealthy
        case .updates:
            return HelmTheme.stateUpdatesReady
        case .active:
            return HelmTheme.stateRunning
        case .review:
            return HelmTheme.stateNeedsReview
        case .error:
            return HelmTheme.stateError
        case .pending, .cached:
            return HelmTheme.stateUnavailable
        }
    }

    private func routeConnectorColor(for tone: WayfinderPopoverRouteTone) -> Color {
        switch tone {
        case .current:
            return HelmTheme.stateHealthy.opacity(0.34)
        case .updates:
            return HelmTheme.stateUpdatesReady.opacity(0.34)
        case .active:
            return HelmTheme.stateRunning.opacity(0.34)
        case .review:
            return HelmTheme.stateNeedsReview.opacity(0.34)
        case .error:
            return HelmTheme.stateError.opacity(0.34)
        case .pending, .cached:
            return HelmTheme.borderSubtle
        }
    }

    private func routeStatusSymbol(for tone: WayfinderPopoverRouteTone) -> String {
        switch tone {
        case .current:
            return "checkmark"
        case .updates:
            return "arrow.up"
        case .active:
            return "arrow.triangle.2.circlepath"
        case .review:
            return "exclamationmark"
        case .error:
            return "xmark"
        case .pending:
            return "ellipsis"
        case .cached:
            return "clock"
        }
    }

    private func routeToneTitleKey(for tone: WayfinderPopoverRouteTone) -> String {
        switch tone {
        case .current:
            return L10n.App.Health.healthy
        case .updates:
            return L10n.App.Health.updatesReady
        case .active:
            return L10n.App.Health.running
        case .review:
            return L10n.App.Health.needsReview
        case .error:
            return L10n.App.Health.error
        case .pending:
            return L10n.Common.loading
        case .cached:
            return "app.popover.wayfinder.route.state.cached"
        }
    }
}

private struct WayfinderPopoverPrimaryButtonStyle: ButtonStyle {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.isEnabled) private var isEnabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 10.5, weight: .semibold))
            .foregroundColor(.white)
            .padding(.horizontal, 13)
            .frame(height: 30)
            .background(
                LinearGradient(
                    colors: [HelmTheme.actionPrimaryDefault, HelmTheme.seaGlass],
                    startPoint: .leading,
                    endPoint: .trailing
                ),
                in: RoundedRectangle(cornerRadius: 8, style: .continuous)
            )
            .opacity(isEnabled ? (configuration.isPressed ? 0.86 : 1) : 0.5)
            .scaleEffect(reduceMotion ? 1 : (configuration.isPressed ? 0.98 : 1))
    }
}

private struct WayfinderPopoverCourseIndicator: View {
    let projection: WayfinderProjectionContent

    @Environment(\.accessibilityDifferentiateWithoutColor) private var differentiateWithoutColor
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Environment(\.colorSchemeContrast) private var contrast

    private var accent: Color {
        switch projection.condition {
        case .healthy:
            return HelmTheme.stateHealthy
        case .updatesReady:
            return HelmTheme.stateUpdatesReady
        case .activeWork, .refreshing:
            return HelmTheme.stateRunning
        case .approvalRequired, .actionableFinding:
            return HelmTheme.stateNeedsReview
        case .failedOrInterrupted:
            return HelmTheme.stateError
        case .offline, .serviceUnavailable:
            return HelmTheme.stateUnavailable
        }
    }

    private var accessibilityValue: String {
        guard let progress = projection.progress else { return "" }
        return NumberFormatter.localizedString(
            from: NSNumber(value: progress.fraction),
            number: .percent
        )
    }

    var body: some View {
        TimelineView(
            .animation(
                minimumInterval: 1.0 / 30.0,
                paused: reduceMotion || !isIndeterminate
            )
        ) { timeline in
            ZStack {
                Circle()
                    .stroke(
                        accent.opacity(contrast == .increased ? 0.3 : 0.17),
                        lineWidth: contrast == .increased ? 10 : 9
                    )

                ring(at: timeline.date)

                Circle()
                    .fill(
                        reduceTransparency
                            ? HelmTheme.surfacePanel
                            : HelmTheme.surfaceElevated.opacity(0.94)
                    )
                    .padding(16)

                centerContent
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityHidden(projection.progress == nil)
        .accessibilityIdentifier("wayfinderPopoverCourseIndicator")
        .accessibilityLabel(projection.title.localized)
        .accessibilityValue(accessibilityValue)
    }

    @ViewBuilder
    private func ring(at date: Date) -> some View {
        switch projection.condition {
        case .healthy:
            Circle()
                .stroke(
                    LinearGradient(
                        colors: [HelmTheme.blue500, HelmTheme.stateHealthy],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    ),
                    style: strokeStyle()
                )
        case let .updatesReady(count):
            let segmentCount = min(max(count, 1), 8)
            ZStack {
                ForEach(0..<segmentCount, id: \.self) { index in
                    Circle()
                        .trim(
                            from: 0.025,
                            to: max(0.08, (1 / CGFloat(segmentCount)) - 0.025)
                        )
                        .stroke(
                            index == 0 ? HelmTheme.blue500 : HelmTheme.stateUpdatesReady,
                            style: strokeStyle()
                        )
                        .rotationEffect(.degrees(Double(index) * (360 / Double(segmentCount)) - 90))
                }
            }
        case .activeWork, .refreshing:
            let rotation = isIndeterminate && !reduceMotion
                ? date.timeIntervalSinceReferenceDate
                    .truncatingRemainder(dividingBy: 1.8) / 1.8 * 360
                : 0
            Circle()
                .trim(from: 0, to: projection.progress?.fraction ?? 0.42)
                .stroke(
                    LinearGradient(
                        colors: [HelmTheme.blue500, HelmTheme.seaGlass],
                        startPoint: .leading,
                        endPoint: .trailing
                    ),
                    style: strokeStyle()
                )
                .rotationEffect(.degrees(-90 + rotation))
        case .approvalRequired, .actionableFinding:
            Circle()
                .trim(from: 0.08, to: 0.76)
                .stroke(accent, style: strokeStyle(dash: differentiateWithoutColor ? [5, 4] : []))
                .rotationEffect(.degrees(-90))
        case .failedOrInterrupted:
            ZStack {
                Circle()
                    .trim(from: 0.03, to: 0.38)
                    .stroke(accent, style: strokeStyle(dash: differentiateWithoutColor ? [4, 3] : []))
                    .rotationEffect(.degrees(-90))
                Circle()
                    .trim(from: 0.52, to: 0.82)
                    .stroke(accent, style: strokeStyle(dash: differentiateWithoutColor ? [4, 3] : []))
                    .rotationEffect(.degrees(-90))
            }
        case .offline, .serviceUnavailable:
            Circle()
                .stroke(accent.opacity(0.78), style: strokeStyle(dash: [3, 6]))
        }
    }

    @ViewBuilder
    private var centerContent: some View {
        switch projection.condition {
        case .healthy:
            Image(systemName: "checkmark")
                .font(.system(size: 27, weight: .semibold))
                .foregroundColor(accent)
        case let .updatesReady(count):
            VStack(spacing: 0) {
                Text("\(count)")
                    .font(.system(size: 24, weight: .bold, design: .rounded))
                    .monospacedDigit()
                Text("app.popover.wayfinder.ready".localized)
                    .font(.system(size: 8.5, weight: .semibold))
                    .foregroundColor(HelmTheme.textSecondary)
            }
        case .activeWork:
            if let progress = projection.progress {
                Text(progress.fraction, format: .percent.precision(.fractionLength(0)))
                    .font(.system(size: 18, weight: .bold, design: .rounded))
                    .monospacedDigit()
            } else {
                Image(systemName: "arrow.triangle.2.circlepath")
                    .font(.system(size: 22, weight: .semibold))
                    .foregroundColor(accent)
            }
        case .refreshing:
            Image(systemName: "arrow.clockwise")
                .font(.system(size: 22, weight: .semibold))
                .foregroundColor(accent)
        case .approvalRequired:
            Image(systemName: "hand.raised.fill")
                .font(.system(size: 23, weight: .semibold))
                .foregroundColor(accent)
        case .actionableFinding:
            Image(systemName: "exclamationmark")
                .font(.system(size: 27, weight: .bold))
                .foregroundColor(accent)
        case .failedOrInterrupted:
            Image(systemName: "xmark")
                .font(.system(size: 24, weight: .bold))
                .foregroundColor(accent)
        case .offline:
            Image(systemName: "wifi.slash")
                .font(.system(size: 22, weight: .semibold))
                .foregroundColor(accent)
        case .serviceUnavailable:
            Image(systemName: "bolt.horizontal.fill")
                .font(.system(size: 21, weight: .semibold))
                .foregroundColor(accent)
        }
    }

    private var isIndeterminate: Bool {
        projection.condition == .refreshing
            || (projection.condition.isActiveWork && projection.progress == nil)
    }

    private func strokeStyle(dash: [CGFloat] = []) -> StrokeStyle {
        StrokeStyle(
            lineWidth: contrast == .increased ? 10 : 9,
            lineCap: .round,
            dash: dash
        )
    }
}

private extension WayfinderCondition {
    var isActiveWork: Bool {
        if case .activeWork = self { return true }
        return false
    }
}

private struct WayfinderPopoverHorizonLines: Shape {
    func path(in rect: CGRect) -> Path {
        var path = Path()
        let spacing: CGFloat = 22
        var x: CGFloat = -rect.height
        while x < rect.width {
            path.move(to: CGPoint(x: x, y: rect.maxY))
            path.addLine(to: CGPoint(x: x + rect.height, y: rect.minY))
            x += spacing
        }
        return path
    }
}
