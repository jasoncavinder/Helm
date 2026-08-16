import SwiftUI

enum V020WayfinderPopoverState: String, CaseIterable {
  case healthy
  case updatesReady = "updates-ready"
  case running
  case needsReview = "needs-review"
  case error
  case offline

  var freshness: String {
    switch self {
    case .healthy, .updatesReady, .needsReview:
      "Checked 2 min ago"
    case .running:
      "Working now"
    case .error:
      "Stopped just now"
    case .offline:
      "Saved 18 min ago"
    }
  }

  var title: String {
    switch self {
    case .healthy:
      "You're on course"
    case .updatesReady:
      "4 updates ready"
    case .running:
      "Updating your environment"
    case .needsReview:
      "One source needs review"
    case .error:
      "An update was interrupted"
    case .offline:
      "Working from saved state"
    }
  }

  var explanation: String {
    switch self {
    case .healthy:
      "Everything Helm checked is current."
    case .updatesReady:
      "Helm has ordered a safe plan for review."
    case .running:
      "5 of 12 steps complete · Applications"
    case .needsReview:
      "mise has more than one active installation."
    case .error:
      "Homebrew apps stopped before verification."
    case .offline:
      "Network actions are paused until you reconnect."
    }
  }

  var primaryAction: String? {
    switch self {
    case .healthy:
      nil
    case .updatesReady:
      "Review Plan"
    case .running:
      "View Activity"
    case .needsReview:
      "Review Environment"
    case .error:
      "Review Recovery"
    case .offline:
      "View Saved State"
    }
  }

  var contextTitle: String {
    switch self {
    case .healthy:
      "Environment covered"
    case .updatesReady:
      "Plan ready"
    case .running:
      "Arc is updating"
    case .needsReview:
      "mise needs a decision"
    case .error:
      "Recovery is available"
    case .offline:
      "Local views remain available"
    }
  }

  var contextDetail: String {
    switch self {
    case .healthy:
      "17 sources · all current"
    case .updatesReady:
      "4 selected · no restart expected"
    case .running:
      "Next: npm packages"
    case .needsReview:
      "3 installations detected"
    case .error:
      "No unverified change was reported"
    case .offline:
      "Library, Plan, and history are still available"
    }
  }

  var contextSymbol: String {
    switch self {
    case .healthy:
      "checkmark.shield"
    case .updatesReady:
      "list.bullet.clipboard"
    case .running:
      "arrow.triangle.2.circlepath"
    case .needsReview:
      "point.3.connected.trianglepath.dotted"
    case .error:
      "arrow.uturn.backward.circle"
    case .offline:
      "internaldrive"
    }
  }
}

struct V020WayfinderPopoverProposal: View {
  let state: V020WayfinderPopoverState

  @Environment(\.colorScheme) private var colorScheme

  private var palette: NavigatorPalette {
    NavigatorPalette(scheme: colorScheme)
  }

  var body: some View {
    VStack(spacing: 0) {
      header
      Divider().overlay(palette.line)
      hero
      routeStrip
      contextRow
      Divider().overlay(palette.line).padding(.horizontal, 17)
      commandRows
    }
    .frame(width: 400, height: 458, alignment: .top)
    .foregroundStyle(palette.primaryText)
    .background {
      ZStack {
        palette.chrome
        LinearGradient(
          colors: [palette.sidebarTop.opacity(0.58), palette.canvas.opacity(0.98)],
          startPoint: .topLeading,
          endPoint: .bottomTrailing
        )
      }
    }
    .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
    .overlay {
      RoundedRectangle(cornerRadius: 18, style: .continuous)
        .stroke(palette.line, lineWidth: 1)
    }
    .shadow(color: .black.opacity(colorScheme == .dark ? 0.5 : 0.2), radius: 24, y: 11)
    .padding(28)
  }

  private var header: some View {
    HStack(spacing: 10) {
      ZStack {
        RoundedRectangle(cornerRadius: 9, style: .continuous)
          .fill(
            LinearGradient(
              colors: [NavigatorPalette.helmBlue, NavigatorPalette.seaGlass],
              startPoint: .topLeading,
              endPoint: .bottomTrailing
            )
          )
        Image(systemName: "helm")
          .font(.system(size: 16, weight: .bold))
          .foregroundStyle(.white)
      }
      .frame(width: 34, height: 34)

      VStack(alignment: .leading, spacing: 1) {
        Text("Helm")
          .font(.system(size: 13, weight: .semibold))
        Text(state.freshness)
          .font(.system(size: 9, weight: .medium))
          .foregroundStyle(palette.secondaryText)
      }

      Spacer()

      ZStack {
        Circle()
          .fill(palette.surface.opacity(0.82))
        Circle()
          .stroke(palette.line, lineWidth: 1)
        Image(systemName: "ellipsis")
          .font(.system(size: 12, weight: .semibold))
          .foregroundStyle(palette.secondaryText)
      }
      .frame(width: 28, height: 28)
      .accessibilityLabel("Utilities")
      .accessibilityHint("Opens Settings, support, app updates, About, and Quit")
    }
    .padding(.horizontal, 17)
    .frame(height: 56)
  }

  private var hero: some View {
    HStack(spacing: 17) {
      V020CourseIndicator(state: state, palette: palette)
        .frame(width: 92, height: 92)

      VStack(alignment: .leading, spacing: 5) {
        Text(state.title)
          .font(.system(size: 19, weight: .semibold, design: .rounded))
          .fixedSize(horizontal: false, vertical: true)

        Text(state.explanation)
          .font(.system(size: 10.5, weight: .medium))
          .foregroundStyle(palette.secondaryText)
          .fixedSize(horizontal: false, vertical: true)

        if let primaryAction = state.primaryAction {
          Text(primaryAction)
            .font(.system(size: 10.5, weight: .semibold))
            .foregroundStyle(.white)
            .padding(.horizontal, 13)
            .frame(height: 30)
            .background(
              LinearGradient(
                colors: [NavigatorPalette.helmBlue, NavigatorPalette.seaGlass],
                startPoint: .leading,
                endPoint: .trailing
              ),
              in: RoundedRectangle(cornerRadius: 8, style: .continuous)
            )
            .padding(.top, 2)
        } else {
          Label("No action needed", systemImage: "checkmark")
            .font(.system(size: 10.5, weight: .semibold))
            .foregroundStyle(NavigatorPalette.seaGlass)
            .padding(.top, 3)
        }
      }

      Spacer(minLength: 0)
    }
    .padding(.horizontal, 19)
    .frame(height: 142)
    .background {
      ZStack {
        LinearGradient(
          colors: [state.accent.opacity(colorScheme == .dark ? 0.12 : 0.08), .clear],
          startPoint: .leading,
          endPoint: .trailing
        )
        V020HorizonLines()
          .stroke(state.accent.opacity(colorScheme == .dark ? 0.09 : 0.11), lineWidth: 1)
      }
      .clipped()
    }
  }

  private var routeStrip: some View {
    HStack(spacing: 0) {
      routeStage("System", symbol: "desktopcomputer", tone: stageTone(for: 0))
      routeConnector(after: 0)
      routeStage("Tools", symbol: "hammer", tone: stageTone(for: 1))
      routeConnector(after: 1)
      routeStage("Apps", symbol: "app.dashed", tone: stageTone(for: 2))
      routeConnector(after: 2)
      routeStage("Packages", symbol: "shippingbox", tone: stageTone(for: 3))
    }
    .padding(.horizontal, 17)
    .frame(height: 67)
    .background(palette.surface.opacity(0.46))
  }

  private func routeStage(_ title: String, symbol: String, tone: V020StageTone) -> some View {
    VStack(spacing: 4) {
      ZStack {
        Circle().fill(tone.color.opacity(0.11))
        Image(systemName: symbol)
          .font(.system(size: 10, weight: .medium))
          .foregroundStyle(tone.color.opacity(0.86))
      }
      .frame(width: 27, height: 27)

      Text(title)
        .font(.system(size: 9, weight: .semibold))
        .foregroundStyle(tone == .cached ? palette.secondaryText : palette.primaryText.opacity(0.7))
    }
    .frame(width: 55)
  }

  private func routeConnector(after index: Int) -> some View {
    Rectangle()
      .fill(connectorColor(after: index))
      .frame(maxWidth: .infinity)
      .frame(height: 1.5)
      .offset(y: -8)
  }

  private var contextRow: some View {
    HStack(spacing: 11) {
      ZStack {
        RoundedRectangle(cornerRadius: 8, style: .continuous)
          .fill(state.accent.opacity(0.12))
        Image(systemName: state.contextSymbol)
          .font(.system(size: 12, weight: .semibold))
          .foregroundStyle(state.accent)
      }
      .frame(width: 31, height: 31)

      VStack(alignment: .leading, spacing: 1) {
        Text(state.contextTitle)
          .font(.system(size: 10.5, weight: .semibold))
        Text(state.contextDetail)
          .font(.system(size: 9.5, weight: .medium))
          .foregroundStyle(palette.secondaryText)
          .lineLimit(1)
      }

      Spacer(minLength: 0)
    }
    .padding(.horizontal, 18)
    .frame(height: 56)
  }

  private var commandRows: some View {
    VStack(spacing: 0) {
      commandRow("Open Dashboard", symbol: "rectangle.3.group", shortcut: "⌘1")
      commandRow("Find software…", symbol: "magnifyingglass", shortcut: "⌘F")
      commandRow(
        state == .offline ? "Check when online" : "Check again",
        symbol: state == .offline ? "wifi.slash" : "arrow.clockwise",
        shortcut: state == .offline ? nil : "⌘R",
        enabled: state != .offline
      )
    }
    .padding(.vertical, 6)
  }

  private func commandRow(
    _ title: String,
    symbol: String,
    shortcut: String?,
    enabled: Bool = true
  ) -> some View {
    HStack(spacing: 11) {
      Image(systemName: symbol)
        .font(.system(size: 12, weight: .medium))
        .foregroundStyle(enabled ? palette.secondaryText : palette.secondaryText.opacity(0.52))
        .frame(width: 18)

      Text(title)
        .font(.system(size: 12, weight: .medium))
        .foregroundStyle(enabled ? palette.primaryText : palette.secondaryText.opacity(0.62))

      Spacer()

      if let shortcut {
        Text(shortcut)
          .font(.system(size: 10, weight: .medium, design: .rounded))
          .foregroundStyle(palette.secondaryText)
      }
    }
    .padding(.horizontal, 18)
    .frame(height: 36)
  }

  private func stageTone(for index: Int) -> V020StageTone {
    switch state {
    case .healthy:
      .current
    case .updatesReady:
      index < 2 ? .current : .updates
    case .running:
      index < 2 ? .current : (index == 2 ? .active : .pending)
    case .needsReview:
      index == 1 ? .review : .current
    case .error:
      index == 2 ? .error : .current
    case .offline:
      .cached
    }
  }

  private func connectorColor(after index: Int) -> Color {
    switch state {
    case .healthy:
      NavigatorPalette.seaGlass.opacity(0.48)
    case .updatesReady:
      index < 2 ? NavigatorPalette.seaGlass.opacity(0.48) : palette.line
    case .running:
      switch index {
      case 0:
        NavigatorPalette.seaGlass.opacity(0.48)
      case 1:
        NavigatorPalette.helmBlue.opacity(0.48)
      default:
        palette.line
      }
    case .needsReview:
      index == 0 ? NavigatorPalette.attention.opacity(0.48) : palette.line
    case .error:
      switch index {
      case 0:
        NavigatorPalette.seaGlass.opacity(0.48)
      case 1:
        Color.red.opacity(0.5)
      default:
        palette.line
      }
    case .offline:
      palette.line
    }
  }
}

private enum V020StageTone: Equatable {
  case current
  case updates
  case active
  case review
  case error
  case pending
  case cached

  var color: Color {
    switch self {
    case .current:
      NavigatorPalette.seaGlass
    case .updates:
      NavigatorPalette.seaGlass
    case .active:
      NavigatorPalette.helmBlue
    case .review:
      NavigatorPalette.attention
    case .error:
      .red
    case .pending, .cached:
      .gray
    }
  }
}

private extension V020WayfinderPopoverState {
  var accent: Color {
    switch self {
    case .healthy, .updatesReady:
      NavigatorPalette.seaGlass
    case .running:
      NavigatorPalette.helmBlue
    case .needsReview:
      NavigatorPalette.attention
    case .error:
      .red
    case .offline:
      .gray
    }
  }
}

private struct V020CourseIndicator: View {
  let state: V020WayfinderPopoverState
  let palette: NavigatorPalette

  var body: some View {
    ZStack {
      Circle()
        .stroke(state.accent.opacity(0.17), lineWidth: 9)

      ring

      Circle()
        .fill(palette.raisedSurface)
        .padding(16)

      centerContent
    }
  }

  @ViewBuilder
  private var ring: some View {
    switch state {
    case .healthy:
      Circle()
        .stroke(
          LinearGradient(
            colors: [NavigatorPalette.helmBlue, NavigatorPalette.seaGlass],
            startPoint: .topLeading,
            endPoint: .bottomTrailing
          ),
          style: StrokeStyle(lineWidth: 9, lineCap: .round)
        )
    case .updatesReady:
      ZStack {
        ForEach(0..<4, id: \.self) { index in
          Circle()
            .trim(from: 0.02, to: 0.135)
            .stroke(
              index == 0 ? NavigatorPalette.helmBlue : NavigatorPalette.seaGlass,
              style: StrokeStyle(lineWidth: 9, lineCap: .round)
            )
            .rotationEffect(.degrees(Double(index) * 90 - 90))
        }
      }
    case .running:
      Circle()
        .trim(from: 0, to: 0.42)
        .stroke(
          LinearGradient(
            colors: [NavigatorPalette.helmBlue, NavigatorPalette.seaGlass],
            startPoint: .leading,
            endPoint: .trailing
          ),
          style: StrokeStyle(lineWidth: 9, lineCap: .round)
        )
        .rotationEffect(.degrees(-90))
    case .needsReview:
      Circle()
        .trim(from: 0.08, to: 0.76)
        .stroke(
          NavigatorPalette.attention,
          style: StrokeStyle(lineWidth: 9, lineCap: .round)
        )
        .rotationEffect(.degrees(-90))
    case .error:
      ZStack {
        Circle()
          .trim(from: 0.03, to: 0.38)
          .stroke(.red, style: StrokeStyle(lineWidth: 9, lineCap: .round))
          .rotationEffect(.degrees(-90))
        Circle()
          .trim(from: 0.52, to: 0.82)
          .stroke(.red, style: StrokeStyle(lineWidth: 9, lineCap: .round))
          .rotationEffect(.degrees(-90))
      }
    case .offline:
      Circle()
        .stroke(
          Color.gray.opacity(0.72),
          style: StrokeStyle(lineWidth: 8, lineCap: .round, dash: [3, 6])
        )
    }
  }

  @ViewBuilder
  private var centerContent: some View {
    switch state {
    case .healthy:
      Image(systemName: "checkmark")
        .font(.system(size: 27, weight: .semibold))
        .foregroundStyle(NavigatorPalette.seaGlass)
    case .updatesReady:
      VStack(spacing: 0) {
        Text("4")
          .font(.system(size: 24, weight: .bold, design: .rounded))
        Text("ready")
          .font(.system(size: 8, weight: .semibold))
          .foregroundStyle(palette.secondaryText)
      }
    case .running:
      VStack(spacing: 0) {
        Text("42%")
          .font(.system(size: 20, weight: .bold, design: .rounded))
        Text("Apps")
          .font(.system(size: 8, weight: .semibold))
          .foregroundStyle(palette.secondaryText)
      }
    case .needsReview:
      Image(systemName: "exclamationmark")
        .font(.system(size: 27, weight: .bold))
        .foregroundStyle(NavigatorPalette.attention)
    case .error:
      Image(systemName: "xmark")
        .font(.system(size: 24, weight: .bold))
        .foregroundStyle(.red)
    case .offline:
      Image(systemName: "wifi.slash")
        .font(.system(size: 22, weight: .semibold))
        .foregroundStyle(.gray)
    }
  }
}

private struct V020HorizonLines: Shape {
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
