import SwiftUI

enum WayfinderRefinement {
  case quieter
  case focused
}

struct WayfinderRefinementDashboard: View {
  let refinement: WayfinderRefinement

  @Environment(\.colorScheme) private var colorScheme

  private var palette: NavigatorPalette { NavigatorPalette(scheme: colorScheme) }

  var body: some View {
    HStack(spacing: 0) {
      sidebar
      Divider().overlay(palette.line)
      VStack(spacing: 0) {
        toolbar
        Divider().overlay(palette.line)
        if refinement == .quieter {
          quieterPage
        } else {
          focusedPage
        }
      }
      .background(palette.canvas)
    }
    .frame(width: 1280, height: 800)
    .foregroundStyle(palette.primaryText)
    .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
    .overlay {
      RoundedRectangle(cornerRadius: 18, style: .continuous).stroke(palette.line, lineWidth: 1)
    }
    .shadow(color: .black.opacity(colorScheme == .dark ? 0.48 : 0.18), radius: 28, y: 14)
    .padding(34)
  }

  private var sidebar: some View {
    VStack(alignment: .leading, spacing: 0) {
      HStack(spacing: 8) {
        Circle().fill(Color(red: 1, green: 0.38, blue: 0.34)).frame(width: 12, height: 12)
        Circle().fill(Color(red: 1, green: 0.74, blue: 0.24)).frame(width: 12, height: 12)
        Circle().fill(Color(red: 0.16, green: 0.78, blue: 0.31)).frame(width: 12, height: 12)
      }
      .padding(.horizontal, 20)
      .frame(height: 52)

      HStack(spacing: 10) {
        ZStack {
          RoundedRectangle(cornerRadius: 10, style: .continuous)
            .fill(
              LinearGradient(
                colors: [NavigatorPalette.helmBlue, NavigatorPalette.seaGlass],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
              )
            )
          Image(systemName: "helm")
            .font(.system(size: 18, weight: .bold))
            .foregroundStyle(.white)
        }
        .frame(width: 38, height: 38)
        VStack(alignment: .leading, spacing: 1) {
          Text("HELM")
            .font(.system(size: 14, weight: .bold, design: .rounded))
            .tracking(1.6)
          Text("Software, on course")
            .font(.system(size: 9, weight: .medium))
            .foregroundStyle(palette.secondaryText)
        }
      }
      .padding(.horizontal, 18)
      .padding(.bottom, 18)

      HStack(spacing: 8) {
        Image(systemName: "magnifyingglass")
        Text("Find anything")
        Spacer()
        Text("⌘K").font(.system(size: 9, weight: .semibold, design: .rounded))
      }
      .font(.system(size: 11))
      .foregroundStyle(palette.secondaryText)
      .padding(.horizontal, 11)
      .frame(height: 34)
      .background(palette.raisedSurface.opacity(0.7), in: RoundedRectangle(cornerRadius: 9))
      .overlay { RoundedRectangle(cornerRadius: 9).stroke(palette.line, lineWidth: 1) }
      .padding(.horizontal, 14)
      .padding(.bottom, 20)

      Text("WORKSPACE")
        .font(.system(size: 8, weight: .bold))
        .tracking(1.1)
        .foregroundStyle(palette.secondaryText)
        .padding(.horizontal, 24)
        .padding(.bottom, 7)

      refinementSidebarRow("Dashboard", symbol: "gauge.with.dots.needle.33percent", selected: true)
      refinementSidebarRow(
        "Plan", symbol: "point.topleft.down.to.point.bottomright.curvepath", badge: "12")
      refinementSidebarRow("Library", symbol: "square.grid.2x2")
      refinementSidebarRow("Activity", symbol: "waveform.path.ecg", live: true)

      Spacer()

      VStack(alignment: .leading, spacing: 8) {
        HStack {
          Label("Environment", systemImage: "point.3.connected.trianglepath.dotted")
          Spacer()
          Image(systemName: "chevron.right")
        }
        .font(.system(size: 11, weight: .semibold))
        HStack(spacing: 7) {
          Circle().fill(NavigatorPalette.seaGlass).frame(width: 7, height: 7)
          Text("9 of 11 sources current")
        }
        .font(.system(size: 9, weight: .medium))
        .foregroundStyle(palette.secondaryText)
      }
      .padding(12)
      .background(palette.raisedSurface.opacity(0.5), in: RoundedRectangle(cornerRadius: 11))
      .overlay { RoundedRectangle(cornerRadius: 11).stroke(palette.line, lineWidth: 1) }
      .padding(.horizontal, 14)

      HStack {
        Label("Updated 2 min ago", systemImage: "clock")
        Spacer()
        Image(systemName: "gearshape")
      }
      .font(.system(size: 9, weight: .medium))
      .foregroundStyle(palette.secondaryText)
      .padding(.horizontal, 20)
      .frame(height: 52)
    }
    .frame(width: 235)
    .background {
      LinearGradient(
        colors: [palette.sidebarTop, palette.sidebarBottom],
        startPoint: .topLeading,
        endPoint: .bottomTrailing
      )
    }
  }

  private func refinementSidebarRow(
    _ title: String,
    symbol: String,
    selected: Bool = false,
    badge: String? = nil,
    live: Bool = false
  ) -> some View {
    HStack(spacing: 11) {
      Image(systemName: symbol).font(.system(size: 14, weight: .medium)).frame(width: 18)
      Text(title).font(.system(size: 12, weight: selected ? .semibold : .regular))
      Spacer()
      if let badge {
        Text(badge)
          .font(.system(size: 9, weight: .semibold, design: .rounded))
          .padding(.horizontal, 7)
          .padding(.vertical, 2)
          .background(palette.raisedSurface, in: Capsule())
      } else if live {
        Circle().fill(NavigatorPalette.seaGlass).frame(width: 7, height: 7)
      }
    }
    .foregroundStyle(selected ? palette.primaryText : palette.secondaryText)
    .padding(.horizontal, 11)
    .frame(height: 39)
    .background {
      if selected {
        RoundedRectangle(cornerRadius: 9, style: .continuous)
          .fill(NavigatorPalette.helmBlue.opacity(colorScheme == .dark ? 0.27 : 0.16))
      }
    }
    .padding(.horizontal, 12)
    .padding(.vertical, 1)
  }

  private var toolbar: some View {
    HStack(spacing: 13) {
      HStack(spacing: 0) {
        Image(systemName: "chevron.left").frame(width: 31, height: 30)
        Divider().frame(height: 17)
        Image(systemName: "chevron.right").opacity(0.3).frame(width: 31, height: 30)
      }
      .foregroundStyle(palette.secondaryText)
      .background(palette.raisedSurface, in: Capsule())
      .overlay { Capsule().stroke(palette.line, lineWidth: 1) }
      Text("Dashboard")
        .font(.system(size: 17, weight: .semibold))
        .fixedSize()
        .layoutPriority(1)
      Spacer()
      Image(systemName: "arrow.clockwise")
        .frame(width: 30, height: 30)
        .background(palette.raisedSurface, in: Circle())
      Image(systemName: "ellipsis")
        .frame(width: 30, height: 30)
        .background(palette.raisedSurface, in: Circle())
    }
    .padding(.horizontal, 20)
    .frame(height: 62)
    .background(palette.chrome)
  }

  private var quieterPage: some View {
    VStack(alignment: .leading, spacing: 20) {
      HStack(alignment: .center, spacing: 24) {
        RefinementCompass(palette: palette, compact: false)
          .frame(width: 132, height: 132)
        VStack(alignment: .leading, spacing: 7) {
          Text("YOUR SOFTWARE ENVIRONMENT")
            .font(.system(size: 9, weight: .bold))
            .tracking(1.2)
            .foregroundStyle(NavigatorPalette.helmBlue)
          Text("Nearly on course")
            .font(.system(size: 30, weight: .semibold, design: .rounded))
          Text("Four updates are ready. Nothing is blocked, and Helm will wait for your approval.")
            .font(.system(size: 12))
            .foregroundStyle(palette.secondaryText)
          HStack(spacing: 9) {
            refinementButton("Review Plan", primary: true)
            refinementButton("Check Again", primary: false)
          }
          .padding(.top, 3)
        }
        Spacer()
      }
      .padding(.horizontal, 27)
      .frame(height: 186)
      .background(
        LinearGradient(
          colors: [
            NavigatorPalette.helmBlue.opacity(colorScheme == .dark ? 0.10 : 0.045),
            NavigatorPalette.seaGlass.opacity(colorScheme == .dark ? 0.06 : 0.025),
          ],
          startPoint: .topLeading,
          endPoint: .bottomTrailing
        ),
        in: RoundedRectangle(cornerRadius: 20, style: .continuous)
      )

      HStack {
        Text("Environment route").font(.system(size: 12, weight: .semibold))
        Spacer()
        Text("214 mapped")
          .font(.system(size: 9, weight: .medium))
          .foregroundStyle(palette.secondaryText)
      }
      quietRoute

      HStack(alignment: .top, spacing: 22) {
        VStack(alignment: .leading, spacing: 0) {
          sectionHeader("Needs you", trailing: "2 items")
          Divider().overlay(palette.line)
          refinementFinding(
            "npm could not verify its refresh", detail: "No packages changed · review evidence",
            tone: NavigatorPalette.attention)
          Divider().overlay(palette.line).padding(.leading, 49)
          refinementFinding(
            "Node 24 and 26 are intentional", detail: "Shims grouped correctly · no action needed",
            tone: NavigatorPalette.helmBlue)
        }
        .frame(maxWidth: .infinity)

        VStack(alignment: .leading, spacing: 10) {
          HStack {
            Label("Refreshing applications", systemImage: "waveform.path.ecg")
              .font(.system(size: 11, weight: .semibold))
            Spacer()
            Text("67%").font(.system(size: 9, weight: .semibold))
          }
          ProposalProgressTrack(progress: 0.67, palette: palette)
          Text("6 of 9 sources checked")
            .font(.system(size: 9))
            .foregroundStyle(palette.secondaryText)
        }
        .padding(.vertical, 2)
        .frame(width: 255, alignment: .topLeading)
      }
      .padding(.horizontal, 4)
    }
    .padding(26)
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
  }

  private var focusedPage: some View {
    VStack(alignment: .leading, spacing: 16) {
      HStack(alignment: .top, spacing: 16) {
        HStack(spacing: 20) {
          RefinementCompass(palette: palette, compact: true)
            .frame(width: 108, height: 108)
          VStack(alignment: .leading, spacing: 6) {
            Text("4 updates are ready")
              .font(.system(size: 27, weight: .semibold, design: .rounded))
            Text("Everything else is on course.")
              .font(.system(size: 14, weight: .medium))
              .foregroundStyle(palette.secondaryText)
            refinementButton("Review Plan", primary: true)
          }
          Spacer()
        }
        .padding(20)
        .frame(maxWidth: .infinity)
        .background(
          LinearGradient(
            colors: [
              NavigatorPalette.helmBlue.opacity(colorScheme == .dark ? 0.19 : 0.10),
              NavigatorPalette.seaGlass.opacity(colorScheme == .dark ? 0.09 : 0.05),
            ],
            startPoint: .topLeading,
            endPoint: .bottomTrailing
          ),
          in: RoundedRectangle(cornerRadius: 18, style: .continuous)
        )
        .overlay {
          RoundedRectangle(cornerRadius: 18).stroke(NavigatorPalette.helmBlue.opacity(0.14))
        }

        VStack(alignment: .leading, spacing: 9) {
          Text("PLAN AT A GLANCE")
            .font(.system(size: 8, weight: .bold))
            .tracking(1)
            .foregroundStyle(NavigatorPalette.helmBlue)
          focusedSummary("12 selected", symbol: "checkmark.circle")
          focusedSummary("1 authorization", symbol: "touchid")
          focusedSummary("No restart", symbol: "power")
          focusedSummary("428 MB", symbol: "arrow.down.circle")
        }
        .padding(18)
        .frame(width: 260, alignment: .topLeading)
        .navigatorSurface(palette, cornerRadius: 18)
      }
      .frame(height: 182)

      compactRoute

      HStack(alignment: .top, spacing: 16) {
        VStack(alignment: .leading, spacing: 0) {
          sectionHeader("Needs you", trailing: "2 items")
          Divider().overlay(palette.line)
          focusedFinding(
            title: "npm refresh needs review", detail: "Verification failed safely",
            action: "Review", tone: NavigatorPalette.attention)
          Divider().overlay(palette.line).padding(.leading, 54)
          focusedFinding(
            title: "Multiple Node versions", detail: "Node 24 and 26 · intentional",
            action: "Details", tone: NavigatorPalette.helmBlue)
        }
        .frame(maxWidth: .infinity)
        .navigatorSurface(palette, cornerRadius: 16)

        VStack(alignment: .leading, spacing: 10) {
          HStack {
            Label("Current activity", systemImage: "waveform.path.ecg")
              .font(.system(size: 11, weight: .semibold))
            Spacer()
            Image(systemName: "chevron.right")
              .font(.system(size: 8, weight: .bold))
              .foregroundStyle(palette.secondaryText)
          }
          Text("Refreshing applications")
            .font(.system(size: 12, weight: .medium))
          ProposalProgressTrack(progress: 0.67, palette: palette)
          HStack {
            Text("6 of 9 sources")
            Spacer()
            Text("67%")
          }
          .font(.system(size: 9))
          .foregroundStyle(palette.secondaryText)
        }
        .padding(16)
        .frame(width: 260, height: 137, alignment: .topLeading)
        .navigatorSurface(palette, cornerRadius: 16)
      }
    }
    .padding(24)
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
  }

  private var quietRoute: some View {
    HStack(spacing: 0) {
      quietRouteStage(
        "System", symbol: "desktopcomputer", detail: "Guarded", tone: NavigatorPalette.seaGlass)
      routeLine
      quietRouteStage(
        "Toolchains", symbol: "hammer", detail: "Current", tone: NavigatorPalette.seaGlass)
      routeLine
      quietRouteStage(
        "Applications", symbol: "app.dashed", detail: "2 ready", tone: NavigatorPalette.attention)
      routeLine
      quietRouteStage(
        "Packages", symbol: "shippingbox", detail: "2 ready", tone: NavigatorPalette.attention)
    }
    .padding(.horizontal, 17)
    .frame(height: 100)
  }

  private var compactRoute: some View {
    HStack(spacing: 0) {
      compactRouteStage(
        "System", symbol: "desktopcomputer", value: "Guarded", tone: NavigatorPalette.seaGlass)
      routeLine
      compactRouteStage(
        "Toolchains", symbol: "hammer", value: "Current", tone: NavigatorPalette.seaGlass)
      routeLine
      compactRouteStage(
        "Applications", symbol: "app.dashed", value: "2 ready", tone: NavigatorPalette.attention)
      routeLine
      compactRouteStage(
        "Packages", symbol: "shippingbox", value: "2 ready", tone: NavigatorPalette.attention)
    }
    .padding(.horizontal, 18)
    .frame(height: 78)
    .navigatorSurface(palette, cornerRadius: 15)
  }

  private var routeLine: some View {
    Rectangle().fill(palette.line).frame(maxWidth: .infinity).frame(height: 2).offset(y: -8)
  }

  private func quietRouteStage(
    _ title: String, symbol: String, detail: String, tone: Color
  ) -> some View {
    VStack(spacing: 5) {
      ZStack {
        Circle().fill(tone.opacity(0.12))
        Image(systemName: symbol).font(.system(size: 14, weight: .medium)).foregroundStyle(tone)
      }
      .frame(width: 36, height: 36)
      Text(title).font(.system(size: 10, weight: .semibold))
      Text(detail).font(.system(size: 8)).foregroundStyle(palette.secondaryText)
    }
    .frame(width: 105)
  }

  private func compactRouteStage(
    _ title: String, symbol: String, value: String, tone: Color
  ) -> some View {
    HStack(spacing: 8) {
      ZStack {
        Circle().fill(tone.opacity(0.13))
        Image(systemName: symbol).font(.system(size: 10, weight: .medium)).foregroundStyle(tone)
      }
      .frame(width: 29, height: 29)
      VStack(alignment: .leading, spacing: 2) {
        Text(title).font(.system(size: 9, weight: .semibold))
        Text(value).font(.system(size: 8)).foregroundStyle(tone)
      }
    }
    .frame(width: 142)
  }

  private func sectionHeader(_ title: String, trailing: String) -> some View {
    HStack {
      Text(title).font(.system(size: 12, weight: .semibold))
      Spacer()
      Text(trailing).font(.system(size: 9, weight: .medium)).foregroundStyle(palette.secondaryText)
    }
    .padding(.horizontal, 14)
    .frame(height: 38)
  }

  private func refinementFinding(_ title: String, detail: String, tone: Color) -> some View {
    HStack(spacing: 11) {
      Circle().fill(tone.opacity(0.15)).frame(width: 30, height: 30).overlay {
        Circle().fill(tone).frame(width: 7, height: 7)
      }
      VStack(alignment: .leading, spacing: 2) {
        Text(title).font(.system(size: 11, weight: .semibold))
        Text(detail).font(.system(size: 9)).foregroundStyle(palette.secondaryText)
      }
      Spacer()
      Image(systemName: "chevron.right")
        .font(.system(size: 8, weight: .bold))
        .foregroundStyle(palette.secondaryText)
    }
    .padding(.horizontal, 14)
    .frame(height: 50)
  }

  private func focusedFinding(
    title: String, detail: String, action: String, tone: Color
  ) -> some View {
    HStack(spacing: 12) {
      ZStack {
        RoundedRectangle(cornerRadius: 8).fill(tone.opacity(0.14))
        Circle().fill(tone).frame(width: 7, height: 7)
      }
      .frame(width: 32, height: 32)
      VStack(alignment: .leading, spacing: 2) {
        Text(title).font(.system(size: 11, weight: .semibold))
        Text(detail).font(.system(size: 9)).foregroundStyle(palette.secondaryText)
      }
      Spacer()
      Text(action)
        .font(.system(size: 9, weight: .semibold))
        .foregroundStyle(tone)
    }
    .padding(.horizontal, 14)
    .frame(height: 52)
  }

  private func focusedSummary(_ title: String, symbol: String) -> some View {
    Label(title, systemImage: symbol)
      .font(.system(size: 10, weight: .medium))
      .foregroundStyle(palette.secondaryText)
  }

  private func refinementButton(_ title: String, primary: Bool) -> some View {
    Text(title)
      .font(.system(size: 10, weight: .semibold))
      .foregroundStyle(primary ? .white : palette.primaryText)
      .padding(.horizontal, 14)
      .frame(height: 31)
      .background(primary ? NavigatorPalette.helmBlue : palette.raisedSurface, in: Capsule())
      .overlay { Capsule().stroke(primary ? .clear : palette.line, lineWidth: 1) }
  }
}

private struct RefinementCompass: View {
  let palette: NavigatorPalette
  let compact: Bool

  var body: some View {
    ZStack {
      Circle().stroke(NavigatorPalette.seaGlass.opacity(0.2), lineWidth: compact ? 9 : 10)
      Circle()
        .trim(from: 0.03, to: 0.77)
        .stroke(
          LinearGradient(
            colors: [NavigatorPalette.helmBlue, NavigatorPalette.seaGlass],
            startPoint: .topLeading,
            endPoint: .bottomTrailing
          ),
          style: StrokeStyle(lineWidth: compact ? 9 : 10, lineCap: .round)
        )
        .rotationEffect(.degrees(-90))
      Circle().fill(palette.raisedSurface).padding(compact ? 18 : 21)
      VStack(spacing: 3) {
        Image(systemName: "helm")
          .font(.system(size: compact ? 25 : 29, weight: .semibold))
          .foregroundStyle(NavigatorPalette.helmBlue)
        Text("4 READY")
          .font(.system(size: 7, weight: .bold, design: .rounded))
          .foregroundStyle(palette.secondaryText)
      }
    }
  }
}

struct WayfinderRefinementPopover: View {
  let refinement: WayfinderRefinement

  @Environment(\.colorScheme) private var colorScheme

  private var palette: NavigatorPalette { NavigatorPalette(scheme: colorScheme) }

  var body: some View {
    VStack(spacing: 0) {
      popoverHeader
      Divider().overlay(palette.line)
      if refinement == .quieter {
        quieterPopoverBody
      } else {
        focusedPopoverBody
      }
      Divider().overlay(palette.line)
      popoverCommands
      popoverFooter
    }
    .frame(width: 400)
    .foregroundStyle(palette.primaryText)
    .background(palette.chrome)
    .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
    .overlay {
      RoundedRectangle(cornerRadius: 18).stroke(palette.line, lineWidth: 1)
    }
    .shadow(color: .black.opacity(colorScheme == .dark ? 0.5 : 0.2), radius: 24, y: 11)
    .padding(28)
  }

  private var popoverHeader: some View {
    HStack(spacing: 9) {
      Image(systemName: "helm")
        .font(.system(size: 15, weight: .bold))
        .foregroundStyle(NavigatorPalette.helmBlue)
      Text("Helm").font(.system(size: 12, weight: .semibold))
      Spacer()
      Circle().fill(NavigatorPalette.seaGlass).frame(width: 7, height: 7)
      Text("Updated 2 min ago")
        .font(.system(size: 8, weight: .medium))
        .foregroundStyle(palette.secondaryText)
    }
    .padding(.horizontal, 17)
    .frame(height: 48)
  }

  private var quieterPopoverBody: some View {
    VStack(alignment: .leading, spacing: 13) {
      HStack(spacing: 15) {
        RefinementCompass(palette: palette, compact: true).frame(width: 76, height: 76)
        VStack(alignment: .leading, spacing: 4) {
          Text("Nearly on course")
            .font(.system(size: 19, weight: .semibold, design: .rounded))
          Text("4 updates are ready")
            .font(.system(size: 10, weight: .medium))
            .foregroundStyle(palette.secondaryText)
          Text("Review Plan  →")
            .font(.system(size: 10, weight: .semibold))
            .foregroundStyle(NavigatorPalette.helmBlue)
        }
      }
      quietPopoverRoute
      HStack {
        Circle().fill(NavigatorPalette.attention).frame(width: 7, height: 7)
        Text("npm refresh needs review").font(.system(size: 9, weight: .medium))
        Spacer()
        Image(systemName: "chevron.right")
          .font(.system(size: 8, weight: .bold))
          .foregroundStyle(palette.secondaryText)
      }
    }
    .padding(17)
    .background(
      LinearGradient(
        colors: [NavigatorPalette.helmBlue.opacity(0.08), NavigatorPalette.seaGlass.opacity(0.04)],
        startPoint: .topLeading,
        endPoint: .bottomTrailing
      )
    )
  }

  private var focusedPopoverBody: some View {
    VStack(alignment: .leading, spacing: 11) {
      Text("NEXT MOVE")
        .font(.system(size: 8, weight: .bold))
        .tracking(1)
        .foregroundStyle(NavigatorPalette.helmBlue)
      HStack(alignment: .firstTextBaseline) {
        Text("4 updates are ready")
          .font(.system(size: 20, weight: .semibold, design: .rounded))
        Spacer()
        Text("Review Plan")
          .font(.system(size: 9, weight: .semibold))
          .foregroundStyle(.white)
          .padding(.horizontal, 12)
          .frame(height: 28)
          .background(NavigatorPalette.helmBlue, in: Capsule())
      }
      Text("Toolchains first · 1 authorization · no restart")
        .font(.system(size: 9))
        .foregroundStyle(palette.secondaryText)
      compactPopoverRoute
      HStack {
        Label("2 findings", systemImage: "exclamationmark.triangle.fill")
          .foregroundStyle(NavigatorPalette.attention)
        Spacer()
        Label("Refresh 67%", systemImage: "waveform.path.ecg")
          .foregroundStyle(NavigatorPalette.seaGlass)
      }
      .font(.system(size: 9, weight: .semibold))
    }
    .padding(17)
    .background(
      LinearGradient(
        colors: [NavigatorPalette.helmBlue.opacity(0.14), NavigatorPalette.seaGlass.opacity(0.06)],
        startPoint: .topLeading,
        endPoint: .bottomTrailing
      )
    )
  }

  private var quietPopoverRoute: some View {
    HStack(spacing: 0) {
      popoverDot("System", tone: NavigatorPalette.seaGlass)
      popoverLine
      popoverDot("Tools", tone: NavigatorPalette.seaGlass)
      popoverLine
      popoverDot("Apps", tone: NavigatorPalette.attention)
      popoverLine
      popoverDot("Pkgs", tone: NavigatorPalette.attention)
    }
  }

  private var compactPopoverRoute: some View {
    HStack(spacing: 8) {
      Text("SYSTEM").foregroundStyle(NavigatorPalette.seaGlass)
      Image(systemName: "chevron.right")
      Text("TOOLS").foregroundStyle(NavigatorPalette.seaGlass)
      Image(systemName: "chevron.right")
      Text("APPS").foregroundStyle(NavigatorPalette.attention)
      Image(systemName: "chevron.right")
      Text("PACKAGES").foregroundStyle(NavigatorPalette.attention)
    }
    .font(.system(size: 7, weight: .bold))
  }

  private func popoverDot(_ title: String, tone: Color) -> some View {
    VStack(spacing: 4) {
      Circle().fill(tone).frame(width: 8, height: 8)
      Text(title).font(.system(size: 7, weight: .semibold)).foregroundStyle(palette.secondaryText)
    }
    .frame(width: 46)
  }

  private var popoverLine: some View {
    Rectangle().fill(palette.line).frame(maxWidth: .infinity).frame(height: 2).offset(y: -6)
  }

  private var popoverCommands: some View {
    VStack(spacing: 0) {
      popoverCommand("Open Dashboard", symbol: "rectangle.3.group", shortcut: "⌘O")
      popoverCommand("Find software…", symbol: "magnifyingglass", shortcut: "⌘K")
      popoverCommand("Check again", symbol: "arrow.clockwise", shortcut: nil)
    }
    .padding(.vertical, 5)
  }

  private func popoverCommand(_ title: String, symbol: String, shortcut: String?) -> some View {
    HStack(spacing: 10) {
      Image(systemName: symbol).frame(width: 17)
      Text(title)
      Spacer()
      if let shortcut {
        Text(shortcut)
      } else {
        Image(systemName: "chevron.right").font(.system(size: 8, weight: .bold))
      }
    }
    .font(.system(size: 10, weight: .medium))
    .foregroundStyle(palette.secondaryText)
    .padding(.horizontal, 18)
    .frame(height: 34)
  }

  private var popoverFooter: some View {
    HStack {
      Text("9 of 11 sources current")
      Spacer()
      Text("Settings…")
      Text("Quit")
    }
    .font(.system(size: 8, weight: .medium))
    .foregroundStyle(palette.secondaryText)
    .padding(.horizontal, 18)
    .frame(height: 40)
    .background(palette.surface)
  }
}
