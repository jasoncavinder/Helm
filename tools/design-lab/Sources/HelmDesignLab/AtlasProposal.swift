import SwiftUI

struct AtlasDashboardProposal: View {
  @Environment(\.colorScheme) private var colorScheme

  private var palette: NavigatorPalette { NavigatorPalette(scheme: colorScheme) }

  var body: some View {
    HStack(spacing: 0) {
      toolRail
      Divider().overlay(palette.line)
      VStack(spacing: 0) {
        toolbar
        Divider().overlay(palette.line)
        HStack(spacing: 0) {
          environmentMap
          Divider().overlay(palette.line)
          actionPanel
        }
      }
    }
    .frame(width: 1280, height: 800)
    .foregroundStyle(palette.primaryText)
    .background(palette.canvas)
    .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
    .overlay {
      RoundedRectangle(cornerRadius: 18, style: .continuous).stroke(palette.line, lineWidth: 1)
    }
    .shadow(color: .black.opacity(colorScheme == .dark ? 0.48 : 0.18), radius: 28, y: 14)
    .padding(34)
  }

  private var toolRail: some View {
    VStack(spacing: 13) {
      HStack(spacing: 8) {
        Circle().fill(Color(red: 1, green: 0.38, blue: 0.34)).frame(width: 10, height: 10)
        Circle().fill(Color(red: 1, green: 0.74, blue: 0.24)).frame(width: 10, height: 10)
        Circle().fill(Color(red: 0.16, green: 0.78, blue: 0.31)).frame(width: 10, height: 10)
      }
      .frame(height: 45)

      ZStack {
        RoundedRectangle(cornerRadius: 12, style: .continuous)
          .fill(
            LinearGradient(
              colors: [NavigatorPalette.helmBlue, NavigatorPalette.seaGlass],
              startPoint: .topLeading,
              endPoint: .bottomTrailing
            )
          )
        Image(systemName: "helm")
          .font(.system(size: 21, weight: .bold))
          .foregroundStyle(.white)
      }
      .frame(width: 43, height: 43)
      .padding(.bottom, 10)

      railButton("point.3.connected.trianglepath.dotted", selected: true)
      railButton("point.topleft.down.to.point.bottomright.curvepath", selected: false)
      railButton("square.grid.2x2", selected: false)
      railButton("waveform.path.ecg", selected: false)
      Spacer()
      railButton("magnifyingglass", selected: false)
      railButton("gearshape", selected: false)
      Text("0.19")
        .font(.system(size: 8, weight: .bold, design: .rounded))
        .foregroundStyle(palette.secondaryText)
        .padding(.bottom, 14)
    }
    .frame(width: 82)
    .background {
      LinearGradient(
        colors: [palette.sidebarTop, palette.sidebarBottom],
        startPoint: .top,
        endPoint: .bottom
      )
    }
  }

  private func railButton(_ symbol: String, selected: Bool) -> some View {
    Image(systemName: symbol)
      .font(.system(size: 15, weight: .medium))
      .foregroundStyle(selected ? .white : palette.secondaryText)
      .frame(width: 39, height: 39)
      .background(
        selected ? NavigatorPalette.helmBlue : .clear, in: RoundedRectangle(cornerRadius: 11))
  }

  private var toolbar: some View {
    HStack {
      VStack(alignment: .leading, spacing: 2) {
        Text("Environment Atlas").font(.system(size: 17, weight: .semibold))
        Text("Your software, from system to package")
          .font(.system(size: 9, weight: .medium))
          .foregroundStyle(palette.secondaryText)
      }
      Spacer()
      HStack(spacing: 7) {
        Circle().fill(NavigatorPalette.seaGlass).frame(width: 7, height: 7)
        Text("Live · 9 sources current")
      }
      .font(.system(size: 10, weight: .medium))
      .foregroundStyle(palette.secondaryText)
      .padding(.horizontal, 12)
      .frame(height: 29)
      .background(palette.surface, in: Capsule())
      Image(systemName: "arrow.clockwise")
        .frame(width: 30, height: 30)
        .background(palette.raisedSurface, in: Circle())
    }
    .padding(.horizontal, 20)
    .frame(height: 62)
    .background(palette.chrome)
  }

  private var environmentMap: some View {
    VStack(alignment: .leading, spacing: 16) {
      HStack(alignment: .firstTextBaseline) {
        VStack(alignment: .leading, spacing: 5) {
          Text("A clear route through your Mac")
            .font(.system(size: 29, weight: .semibold, design: .rounded))
          Text("Select any stage to inspect coverage, ownership, updates, and recovery.")
            .font(.system(size: 11))
            .foregroundStyle(palette.secondaryText)
        }
        Spacer()
        Text("214 mapped")
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(palette.secondaryText)
      }

      ZStack {
        RoundedRectangle(cornerRadius: 22, style: .continuous)
          .fill(
            LinearGradient(
              colors: [
                NavigatorPalette.helmBlue.opacity(colorScheme == .dark ? 0.14 : 0.065),
                NavigatorPalette.seaGlass.opacity(colorScheme == .dark ? 0.09 : 0.04),
              ],
              startPoint: .topLeading,
              endPoint: .bottomTrailing
            )
          )
        AtlasConnections(palette: palette)
        VStack(spacing: 31) {
          atlasStage(
            symbol: "desktopcomputer", title: "System", detail: "macOS · guarded authority",
            count: "1 available", tone: NavigatorPalette.seaGlass, width: 380)
          HStack(spacing: 52) {
            atlasStage(
              symbol: "hammer", title: "Toolchains", detail: "mise · rustup · Xcode",
              count: "Current", tone: NavigatorPalette.seaGlass, width: 300)
            atlasStage(
              symbol: "app.dashed", title: "Applications", detail: "Homebrew · Sparkle · MAS",
              count: "2 ready", tone: NavigatorPalette.attention, width: 300)
          }
          atlasStage(
            symbol: "shippingbox", title: "Packages", detail: "npm · Cargo · RubyGems",
            count: "2 ready", tone: NavigatorPalette.attention, width: 380)
        }
        .padding(28)
      }
      .frame(maxHeight: .infinity)

      HStack(spacing: 18) {
        Label("Authority ordered", systemImage: "list.number")
        Label("Changes require approval", systemImage: "hand.raised")
        Label("Every action gets a receipt", systemImage: "doc.text.magnifyingglass")
      }
      .font(.system(size: 9, weight: .medium))
      .foregroundStyle(palette.secondaryText)
    }
    .padding(26)
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
  }

  private func atlasStage(
    symbol: String, title: String, detail: String, count: String, tone: Color, width: CGFloat
  ) -> some View {
    HStack(spacing: 14) {
      ZStack {
        RoundedRectangle(cornerRadius: 12).fill(tone.opacity(0.13))
        Image(systemName: symbol).font(.system(size: 18, weight: .medium)).foregroundStyle(tone)
      }
      .frame(width: 45, height: 45)
      VStack(alignment: .leading, spacing: 3) {
        Text(title).font(.system(size: 13, weight: .semibold))
        Text(detail).font(.system(size: 9)).foregroundStyle(palette.secondaryText)
      }
      Spacer()
      Text(count)
        .font(.system(size: 9, weight: .semibold))
        .foregroundStyle(tone)
      Image(systemName: "chevron.right")
        .font(.system(size: 8, weight: .bold))
        .foregroundStyle(palette.secondaryText)
    }
    .padding(.horizontal, 15)
    .frame(width: width, height: 72)
    .background(palette.raisedSurface, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
    .overlay {
      RoundedRectangle(cornerRadius: 16, style: .continuous).stroke(
        tone.opacity(0.24), lineWidth: 1)
    }
    .shadow(color: .black.opacity(0.07), radius: 7, y: 3)
  }

  private var actionPanel: some View {
    VStack(alignment: .leading, spacing: 18) {
      Text("NEXT MOVE")
        .font(.system(size: 9, weight: .bold))
        .tracking(1.2)
        .foregroundStyle(NavigatorPalette.helmBlue)

      VStack(alignment: .leading, spacing: 8) {
        Text("4 updates are ready")
          .font(.system(size: 23, weight: .semibold, design: .rounded))
        Text(
          "Helm has prepared an authority-ordered route with one authorization and no expected restart."
        )
        .font(.system(size: 10))
        .foregroundStyle(palette.secondaryText)
        .fixedSize(horizontal: false, vertical: true)
        Text("Review Plan  →")
          .font(.system(size: 11, weight: .semibold))
          .foregroundStyle(.white)
          .padding(.horizontal, 14)
          .frame(height: 31)
          .background(NavigatorPalette.helmBlue, in: Capsule())
      }

      Divider().overlay(palette.line)

      Text("NEEDS YOU")
        .font(.system(size: 9, weight: .bold))
        .tracking(1.1)
        .foregroundStyle(palette.secondaryText)
      atlasFinding(
        "npm refresh", detail: "Verification failed safely", tone: NavigatorPalette.attention)
      atlasFinding(
        "Multiple Node versions", detail: "Intentional · grouped", tone: NavigatorPalette.helmBlue)

      Divider().overlay(palette.line)

      VStack(alignment: .leading, spacing: 8) {
        HStack {
          Label("Refreshing apps", systemImage: "waveform.path.ecg")
            .font(.system(size: 11, weight: .semibold))
          Spacer()
          Text("67%").font(.system(size: 9, weight: .semibold))
        }
        ProposalProgressTrack(progress: 0.67, palette: palette)
        Text("6 of 9 sources checked")
          .font(.system(size: 9))
          .foregroundStyle(palette.secondaryText)
      }

      Spacer()

      HStack {
        Image(systemName: "point.3.connected.trianglepath.dotted")
        Text("Environment details")
        Spacer()
        Image(systemName: "chevron.right")
      }
      .font(.system(size: 10, weight: .semibold))
      .foregroundStyle(palette.secondaryText)
    }
    .padding(22)
    .frame(width: 292, alignment: .topLeading)
    .frame(maxHeight: .infinity, alignment: .topLeading)
    .background(palette.chrome)
  }

  private func atlasFinding(_ title: String, detail: String, tone: Color) -> some View {
    HStack(spacing: 10) {
      Circle().fill(tone).frame(width: 7, height: 7)
      VStack(alignment: .leading, spacing: 2) {
        Text(title).font(.system(size: 10, weight: .semibold))
        Text(detail).font(.system(size: 9)).foregroundStyle(palette.secondaryText)
      }
      Spacer()
      Image(systemName: "chevron.right")
        .font(.system(size: 8, weight: .bold))
        .foregroundStyle(palette.secondaryText)
    }
  }
}

private struct AtlasConnections: View {
  let palette: NavigatorPalette

  var body: some View {
    GeometryReader { geometry in
      Path { path in
        let center = geometry.size.width / 2
        path.move(to: CGPoint(x: center, y: 112))
        path.addLine(to: CGPoint(x: center, y: 176))
        path.move(to: CGPoint(x: geometry.size.width * 0.28, y: 176))
        path.addLine(to: CGPoint(x: geometry.size.width * 0.72, y: 176))
        path.move(to: CGPoint(x: geometry.size.width * 0.28, y: 176))
        path.addLine(to: CGPoint(x: geometry.size.width * 0.28, y: 225))
        path.move(to: CGPoint(x: geometry.size.width * 0.72, y: 176))
        path.addLine(to: CGPoint(x: geometry.size.width * 0.72, y: 225))
        path.move(to: CGPoint(x: geometry.size.width * 0.28, y: 297))
        path.addLine(to: CGPoint(x: geometry.size.width * 0.28, y: 344))
        path.move(to: CGPoint(x: geometry.size.width * 0.72, y: 297))
        path.addLine(to: CGPoint(x: geometry.size.width * 0.72, y: 344))
        path.move(to: CGPoint(x: geometry.size.width * 0.28, y: 344))
        path.addLine(to: CGPoint(x: geometry.size.width * 0.72, y: 344))
        path.move(to: CGPoint(x: center, y: 344))
        path.addLine(to: CGPoint(x: center, y: 390))
      }
      .stroke(
        LinearGradient(
          colors: [NavigatorPalette.seaGlass.opacity(0.5), NavigatorPalette.helmBlue.opacity(0.4)],
          startPoint: .top,
          endPoint: .bottom
        ),
        style: StrokeStyle(lineWidth: 2, dash: [5, 5])
      )
    }
  }
}

struct AtlasPopoverProposal: View {
  @Environment(\.colorScheme) private var colorScheme

  private var palette: NavigatorPalette { NavigatorPalette(scheme: colorScheme) }

  var body: some View {
    VStack(spacing: 0) {
      HStack {
        Label("Environment", systemImage: "point.3.connected.trianglepath.dotted")
          .font(.system(size: 12, weight: .semibold))
        Spacer()
        Circle().fill(NavigatorPalette.seaGlass).frame(width: 7, height: 7)
        Text("Live").font(.system(size: 9, weight: .medium)).foregroundStyle(palette.secondaryText)
      }
      .padding(.horizontal, 18)
      .frame(height: 49)

      HStack(spacing: 0) {
        atlasMiniStage("System", symbol: "desktopcomputer", tone: NavigatorPalette.seaGlass)
        miniConnector
        atlasMiniStage("Tools", symbol: "hammer", tone: NavigatorPalette.seaGlass)
        miniConnector
        atlasMiniStage("Apps", symbol: "app.dashed", tone: NavigatorPalette.attention)
        miniConnector
        atlasMiniStage("Pkgs", symbol: "shippingbox", tone: NavigatorPalette.attention)
      }
      .padding(.horizontal, 16)
      .padding(.vertical, 15)
      .background(
        LinearGradient(
          colors: [
            NavigatorPalette.helmBlue.opacity(0.13), NavigatorPalette.seaGlass.opacity(0.07),
          ],
          startPoint: .topLeading,
          endPoint: .bottomTrailing
        )
      )

      VStack(alignment: .leading, spacing: 8) {
        Text("YOUR NEXT MOVE")
          .font(.system(size: 8, weight: .bold))
          .tracking(1)
          .foregroundStyle(NavigatorPalette.helmBlue)
        Text("Review 4 ready updates")
          .font(.system(size: 20, weight: .semibold, design: .rounded))
        Text("Toolchains first · 1 authorization · no restart")
          .font(.system(size: 9))
          .foregroundStyle(palette.secondaryText)
        HStack {
          Text("Review Plan  →")
            .font(.system(size: 10, weight: .semibold))
            .foregroundStyle(.white)
            .padding(.horizontal, 14)
            .frame(height: 30)
            .background(NavigatorPalette.helmBlue, in: Capsule())
          Spacer()
          Text("2 findings")
            .font(.system(size: 9, weight: .semibold))
            .foregroundStyle(NavigatorPalette.attention)
        }
      }
      .padding(18)

      Divider().overlay(palette.line)

      HStack {
        Text("Open Atlas")
        Spacer()
        Text("Find software…")
        Image(systemName: "gearshape")
      }
      .font(.system(size: 10, weight: .semibold))
      .foregroundStyle(palette.secondaryText)
      .padding(.horizontal, 18)
      .frame(height: 45)
      .background(palette.surface)
    }
    .frame(width: 420)
    .foregroundStyle(palette.primaryText)
    .background(palette.chrome)
    .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
    .overlay {
      RoundedRectangle(cornerRadius: 18, style: .continuous).stroke(palette.line, lineWidth: 1)
    }
    .shadow(color: .black.opacity(colorScheme == .dark ? 0.5 : 0.2), radius: 24, y: 11)
    .padding(28)
  }

  private func atlasMiniStage(_ title: String, symbol: String, tone: Color) -> some View {
    VStack(spacing: 5) {
      ZStack {
        Circle().fill(tone.opacity(0.16))
        Image(systemName: symbol).font(.system(size: 10, weight: .medium)).foregroundStyle(tone)
      }
      .frame(width: 30, height: 30)
      Text(title).font(.system(size: 8, weight: .semibold)).foregroundStyle(palette.secondaryText)
    }
    .frame(width: 57)
  }

  private var miniConnector: some View {
    Rectangle().fill(palette.line).frame(maxWidth: .infinity).frame(height: 2).offset(y: -8)
  }
}
