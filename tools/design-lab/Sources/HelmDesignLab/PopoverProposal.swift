import SwiftUI

struct PopoverProposal: View {
  enum Mode {
    case attention
    case active
  }

  let mode: Mode

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
      Divider().overlay(palette.line).padding(.horizontal, 17)
      commandRows
      footer
    }
    .frame(width: 400)
    .foregroundStyle(palette.primaryText)
    .background {
      ZStack {
        palette.chrome
        LinearGradient(
          colors: [palette.sidebarTop.opacity(0.64), palette.canvas.opacity(0.97)],
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
        Text("Helm").font(.system(size: 13, weight: .semibold))
        Text(mode == .active ? "Working now" : "Updated 2 min ago")
          .font(.system(size: 9, weight: .medium))
          .foregroundStyle(palette.secondaryText)
      }
      Spacer()
      Image(systemName: "ellipsis.circle")
        .font(.system(size: 16, weight: .medium))
        .foregroundStyle(palette.secondaryText)
        .accessibilityLabel("More")
    }
    .padding(.horizontal, 17)
    .frame(height: 56)
  }

  @ViewBuilder
  private var hero: some View {
    switch mode {
    case .attention:
      attentionHero
    case .active:
      activeHero
    }
  }

  private var attentionHero: some View {
    HStack(spacing: 17) {
      MiniCompass(palette: palette, mode: .attention)
        .frame(width: 79, height: 79)
      VStack(alignment: .leading, spacing: 5) {
        Text("4 updates are ready")
          .font(.system(size: 20, weight: .semibold, design: .rounded))
        Text("Everything else is on course.")
          .font(.system(size: 11, weight: .medium))
          .foregroundStyle(palette.secondaryText)
        Text("Review Plan")
          .font(.system(size: 11, weight: .semibold))
          .foregroundStyle(.white)
          .padding(.horizontal, 13)
          .frame(height: 30)
          .background(
            LinearGradient(
              colors: [NavigatorPalette.helmBlue, NavigatorPalette.seaGlass],
              startPoint: .leading,
              endPoint: .trailing
            ),
            in: RoundedRectangle(cornerRadius: 8)
          )
          .padding(.top, 2)
      }
      Spacer()
    }
    .padding(.horizontal, 19)
    .padding(.vertical, 18)
  }

  private var activeHero: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack(spacing: 15) {
        MiniCompass(palette: palette, mode: .active)
          .frame(width: 70, height: 70)
        VStack(alignment: .leading, spacing: 4) {
          Text("Updating your environment")
            .font(.system(size: 18, weight: .semibold, design: .rounded))
          Text("5 of 12 complete · Packages")
            .font(.system(size: 10, weight: .medium))
            .foregroundStyle(palette.secondaryText)
        }
      }
      ProposalProgressTrack(progress: 0.42, palette: palette)
      HStack {
        VStack(alignment: .leading, spacing: 2) {
          Text("Now: Homebrew applications")
            .font(.system(size: 10, weight: .semibold))
          Text("Next: npm packages")
            .font(.system(size: 9))
            .foregroundStyle(palette.secondaryText)
        }
        Spacer()
        Text("View Activity")
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(NavigatorPalette.horizon)
      }
    }
    .padding(.horizontal, 19)
    .padding(.vertical, 17)
  }

  private var routeStrip: some View {
    HStack(spacing: 0) {
      miniStage("System", symbol: "desktopcomputer", state: .complete)
      connector(complete: true)
      miniStage("Tools", symbol: "hammer", state: mode == .active ? .complete : .current)
      connector(complete: mode == .active)
      miniStage("Apps", symbol: "app.dashed", state: mode == .active ? .current : .attention)
      connector(complete: false)
      miniStage("Packages", symbol: "shippingbox", state: .attention)
    }
    .padding(.horizontal, 17)
    .padding(.vertical, 13)
    .background(palette.surface.opacity(0.72))
  }

  private enum MiniStageState {
    case complete
    case current
    case attention
  }

  private func miniStage(_ title: String, symbol: String, state: MiniStageState) -> some View {
    let tone: Color =
      switch state {
      case .complete: NavigatorPalette.seaGlass
      case .current: NavigatorPalette.helmBlue
      case .attention: NavigatorPalette.attention
      }

    return VStack(spacing: 4) {
      ZStack {
        Circle().fill(tone.opacity(0.15))
        Image(systemName: symbol).font(.system(size: 10, weight: .medium)).foregroundStyle(tone)
      }
      .frame(width: 27, height: 27)
      Text(title).font(.system(size: 8, weight: .semibold)).foregroundStyle(palette.secondaryText)
    }
    .frame(width: 55)
  }

  private func connector(complete: Bool) -> some View {
    Rectangle()
      .fill(complete ? NavigatorPalette.seaGlass.opacity(0.55) : palette.line)
      .frame(maxWidth: .infinity)
      .frame(height: 2)
      .offset(y: -8)
  }

  private var commandRows: some View {
    VStack(spacing: 0) {
      commandRow("Open Dashboard", symbol: "rectangle.3.group", shortcut: "⌘O")
      commandRow("Find software…", symbol: "magnifyingglass", shortcut: "⌘K")
      commandRow("Check again", symbol: "arrow.clockwise", shortcut: nil)
    }
    .padding(.vertical, 7)
  }

  private func commandRow(_ title: String, symbol: String, shortcut: String?) -> some View {
    HStack(spacing: 11) {
      Image(systemName: symbol)
        .font(.system(size: 12, weight: .medium))
        .foregroundStyle(palette.secondaryText)
        .frame(width: 18)
      Text(title).font(.system(size: 12, weight: .medium))
      Spacer()
      if let shortcut {
        Text(shortcut)
          .font(.system(size: 10, weight: .medium, design: .rounded))
          .foregroundStyle(palette.secondaryText)
      } else {
        Image(systemName: "chevron.right")
          .font(.system(size: 8, weight: .bold))
          .foregroundStyle(palette.secondaryText.opacity(0.7))
      }
    }
    .padding(.horizontal, 18)
    .frame(height: 37)
  }

  private var footer: some View {
    HStack(spacing: 13) {
      Text("9 of 11 sources current")
        .font(.system(size: 9, weight: .medium))
        .foregroundStyle(palette.secondaryText)
      Spacer()
      Text("Settings…")
      Text("Quit")
    }
    .font(.system(size: 9, weight: .medium))
    .foregroundStyle(palette.secondaryText)
    .padding(.horizontal, 18)
    .frame(height: 43)
    .background(palette.surface.opacity(0.7))
  }
}

private struct MiniCompass: View {
  enum Mode {
    case attention
    case active
  }

  let palette: NavigatorPalette
  let mode: Mode

  var body: some View {
    ZStack {
      Circle()
        .stroke(
          mode == .active
            ? NavigatorPalette.seaGlass.opacity(0.22) : NavigatorPalette.attention.opacity(0.22),
          lineWidth: 8
        )
      Circle()
        .trim(from: 0.03, to: mode == .active ? 0.45 : 0.78)
        .stroke(
          mode == .active
            ? LinearGradient(
              colors: [NavigatorPalette.seaGlass, NavigatorPalette.helmBlue], startPoint: .leading,
              endPoint: .trailing)
            : LinearGradient(
              colors: [NavigatorPalette.attention, NavigatorPalette.helmBlue], startPoint: .leading,
              endPoint: .trailing),
          style: StrokeStyle(lineWidth: 8, lineCap: .round)
        )
        .rotationEffect(.degrees(-90))
      Circle().fill(palette.raisedSurface).padding(14)
      Image(systemName: mode == .active ? "arrow.triangle.2.circlepath" : "helm")
        .font(.system(size: 24, weight: .semibold))
        .foregroundStyle(mode == .active ? NavigatorPalette.seaGlass : NavigatorPalette.helmBlue)
    }
  }
}
