import SwiftUI

struct NavigatorPalette {
  let canvas: Color
  let chrome: Color
  let sidebarTop: Color
  let sidebarBottom: Color
  let surface: Color
  let raisedSurface: Color
  let line: Color
  let primaryText: Color
  let secondaryText: Color

  static let helmBlue = Color(red: 0.055, green: 0.40, blue: 0.71)
  static let seaGlass = Color(red: 0.08, green: 0.62, blue: 0.60)
  static let horizon = Color(red: 0.19, green: 0.67, blue: 0.82)
  static let attention = Color(red: 0.91, green: 0.55, blue: 0.14)

  init(scheme: ColorScheme) {
    let isDark = scheme == .dark
    canvas =
      isDark
      ? Color(red: 0.055, green: 0.065, blue: 0.078)
      : Color(red: 0.965, green: 0.972, blue: 0.977)
    chrome =
      isDark
      ? Color(red: 0.075, green: 0.086, blue: 0.102)
      : Color.white.opacity(0.91)
    sidebarTop =
      isDark
      ? Color(red: 0.075, green: 0.11, blue: 0.14)
      : Color(red: 0.86, green: 0.93, blue: 0.955)
    sidebarBottom =
      isDark
      ? Color(red: 0.055, green: 0.075, blue: 0.095)
      : Color(red: 0.925, green: 0.95, blue: 0.955)
    surface = isDark ? Color.white.opacity(0.055) : Color.white.opacity(0.72)
    raisedSurface = isDark ? Color.white.opacity(0.09) : Color.white.opacity(0.96)
    line = isDark ? Color.white.opacity(0.12) : Color.black.opacity(0.095)
    primaryText = isDark ? Color.white.opacity(0.95) : Color.black.opacity(0.86)
    secondaryText = isDark ? Color.white.opacity(0.60) : Color.black.opacity(0.53)
  }
}

enum DashboardDestination: String, CaseIterable {
  case dashboard = "Dashboard"
  case plan = "Plan"
  case library = "Library"
  case activity = "Activity"

  var symbol: String {
    switch self {
    case .dashboard: "gauge.with.dots.needle.33percent"
    case .plan: "point.topleft.down.to.point.bottomright.curvepath"
    case .library: "square.grid.2x2"
    case .activity: "waveform.path.ecg"
    }
  }
}

enum ProposalArtifact: String, CaseIterable {
  case dashboardOverview = "dashboard-overview"
  case dashboardPlan = "dashboard-plan"
  case dashboardLibrary = "dashboard-library"
  case popoverAttention = "popover-attention"
  case popoverActive = "popover-active"
  case briefingDashboard = "briefing-dashboard"
  case briefingPopover = "briefing-popover"
  case atlasDashboard = "atlas-dashboard"
  case atlasPopover = "atlas-popover"

  var schemes: [ColorScheme] {
    switch self {
    case .dashboardOverview: [.light, .dark]
    case .dashboardPlan, .dashboardLibrary, .popoverAttention, .briefingDashboard,
      .briefingPopover, .atlasDashboard:
      [.light]
    case .popoverActive, .atlasPopover: [.dark]
    }
  }
}

extension View {
  func navigatorSurface(_ palette: NavigatorPalette, cornerRadius: CGFloat = 14) -> some View {
    background(
      palette.surface, in: RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
    )
    .overlay {
      RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
        .stroke(palette.line, lineWidth: 1)
    }
  }
}

struct ProposalProgressTrack: View {
  let progress: CGFloat
  let palette: NavigatorPalette

  var body: some View {
    GeometryReader { geometry in
      ZStack(alignment: .leading) {
        Capsule().fill(palette.line)
        Capsule()
          .fill(
            LinearGradient(
              colors: [NavigatorPalette.helmBlue, NavigatorPalette.seaGlass],
              startPoint: .leading,
              endPoint: .trailing
            )
          )
          .frame(width: geometry.size.width * progress)
      }
    }
    .frame(height: 6)
  }
}
