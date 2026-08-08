import SwiftUI

enum DesignDirection: String, CaseIterable {
  case quietNative = "quiet-native"
  case operationsDesk = "operations-desk"
  case guidedClarity = "guided-clarity"

  var name: String {
    switch self {
    case .quietNative: "Quiet Native"
    case .operationsDesk: "Operations Desk"
    case .guidedClarity: "Guided Clarity"
    }
  }

  var accent: Color {
    switch self {
    case .quietNative: Color(red: 0.10, green: 0.42, blue: 0.76)
    case .operationsDesk: Color(red: 0.08, green: 0.55, blue: 0.49)
    case .guidedClarity: Color(red: 0.06, green: 0.49, blue: 0.69)
    }
  }

  var warmAccent: Color {
    switch self {
    case .quietNative: Color(red: 0.86, green: 0.55, blue: 0.16)
    case .operationsDesk: Color(red: 0.91, green: 0.62, blue: 0.20)
    case .guidedClarity: Color(red: 0.91, green: 0.48, blue: 0.20)
    }
  }
}

struct ProposalPalette {
  let canvas: Color
  let chrome: Color
  let sidebar: Color
  let surface: Color
  let raisedSurface: Color
  let line: Color
  let primaryText: Color
  let secondaryText: Color

  init(direction: DesignDirection, scheme: ColorScheme) {
    let isDark = scheme == .dark
    switch direction {
    case .quietNative:
      canvas =
        isDark
        ? Color(red: 0.075, green: 0.082, blue: 0.095) : Color(red: 0.94, green: 0.955, blue: 0.97)
      chrome =
        isDark
        ? Color(red: 0.105, green: 0.115, blue: 0.13) : Color(red: 0.975, green: 0.98, blue: 0.985)
      sidebar =
        isDark
        ? Color(red: 0.09, green: 0.10, blue: 0.115) : Color(red: 0.91, green: 0.93, blue: 0.95)
    case .operationsDesk:
      canvas =
        isDark
        ? Color(red: 0.045, green: 0.065, blue: 0.07) : Color(red: 0.93, green: 0.95, blue: 0.945)
      chrome =
        isDark
        ? Color(red: 0.065, green: 0.085, blue: 0.09) : Color(red: 0.965, green: 0.975, blue: 0.97)
      sidebar =
        isDark
        ? Color(red: 0.035, green: 0.055, blue: 0.06) : Color(red: 0.865, green: 0.90, blue: 0.89)
    case .guidedClarity:
      canvas =
        isDark
        ? Color(red: 0.065, green: 0.075, blue: 0.09) : Color(red: 0.955, green: 0.95, blue: 0.925)
      chrome =
        isDark
        ? Color(red: 0.09, green: 0.105, blue: 0.12) : Color(red: 0.99, green: 0.985, blue: 0.965)
      sidebar =
        isDark
        ? Color(red: 0.075, green: 0.09, blue: 0.105) : Color(red: 0.92, green: 0.93, blue: 0.91)
    }

    surface = isDark ? Color.white.opacity(0.055) : Color.white.opacity(0.72)
    raisedSurface = isDark ? Color.white.opacity(0.085) : Color.white.opacity(0.94)
    line = isDark ? Color.white.opacity(0.105) : Color.black.opacity(0.10)
    primaryText = isDark ? Color.white.opacity(0.94) : Color.black.opacity(0.86)
    secondaryText = isDark ? Color.white.opacity(0.59) : Color.black.opacity(0.55)
  }
}
