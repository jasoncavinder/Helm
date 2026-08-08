import SwiftUI

struct ControlCenterProposal: View {
  let direction: DesignDirection

  @Environment(\.colorScheme) private var colorScheme

  private var palette: ProposalPalette {
    ProposalPalette(direction: direction, scheme: colorScheme)
  }

  var body: some View {
    ZStack {
      palette.canvas
      LinearGradient(
        colors: [direction.accent.opacity(colorScheme == .dark ? 0.10 : 0.07), .clear],
        startPoint: .topLeading,
        endPoint: .center
      )

      VStack(spacing: 0) {
        titlebar
        Divider().overlay(palette.line)
        HStack(spacing: 0) {
          sidebar
          Divider().overlay(palette.line)
          mainContent
          Divider().overlay(palette.line)
          inspector
        }
      }
    }
    .frame(width: 1280, height: 800)
    .foregroundStyle(palette.primaryText)
    .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
    .overlay {
      RoundedRectangle(cornerRadius: 14, style: .continuous)
        .stroke(palette.line, lineWidth: 1)
    }
    .shadow(color: .black.opacity(colorScheme == .dark ? 0.45 : 0.19), radius: 26, y: 14)
    .padding(34)
    .background(Color.clear)
  }

  private var titlebar: some View {
    HStack(spacing: 16) {
      HStack(spacing: 8) {
        Circle().fill(Color(red: 1.0, green: 0.38, blue: 0.34)).frame(width: 12, height: 12)
        Circle().fill(Color(red: 1.0, green: 0.74, blue: 0.24)).frame(width: 12, height: 12)
        Circle().fill(Color(red: 0.16, green: 0.78, blue: 0.31)).frame(width: 12, height: 12)
      }
      .padding(.trailing, 4)

      toolbarGlyph("sidebar.left", label: "Toggle sidebar")
      HStack(spacing: 9) {
        Image(systemName: "helm")
          .font(.system(size: 17, weight: .semibold))
          .foregroundStyle(direction.accent)
        Text("Health")
          .font(.system(size: 15, weight: .semibold))
      }

      Spacer()
      HStack(spacing: 8) {
        Image(systemName: "magnifyingglass")
          .foregroundStyle(palette.secondaryText)
        Text("Search Helm")
          .foregroundStyle(palette.secondaryText)
        Spacer(minLength: 16)
        Text("⌘F")
          .font(.system(size: 11, weight: .medium, design: .rounded))
          .foregroundStyle(palette.secondaryText.opacity(0.78))
      }
      .font(.system(size: 13))
      .padding(.horizontal, 11)
      .frame(width: direction == .operationsDesk ? 230 : 260, height: 30)
      .background(palette.surface, in: RoundedRectangle(cornerRadius: 7, style: .continuous))
      .overlay {
        RoundedRectangle(cornerRadius: 7, style: .continuous)
          .stroke(palette.line, lineWidth: 1)
      }

      toolbarGlyph("arrow.clockwise", label: "Refresh")
      toolbarGlyph("sidebar.right", label: "Toggle inspector")
    }
    .frame(height: 52)
    .padding(.horizontal, 18)
    .background(palette.chrome)
  }

  private func toolbarGlyph(_ symbol: String, label: String) -> some View {
    Image(systemName: symbol)
      .font(.system(size: 14, weight: .medium))
      .frame(width: 30, height: 28)
      .background(palette.surface, in: RoundedRectangle(cornerRadius: 6, style: .continuous))
      .overlay {
        RoundedRectangle(cornerRadius: 6, style: .continuous)
          .stroke(palette.line, lineWidth: 1)
      }
      .accessibilityLabel(label)
  }

  private var sidebar: some View {
    VStack(alignment: .leading, spacing: direction == .operationsDesk ? 2 : 5) {
      brandLockup.padding(.bottom, direction == .operationsDesk ? 10 : 17)
      sidebarRow("heart.text.square", "Health", selected: true)
      sidebarRow("arrow.down.circle", "Updates", badge: "12")
      sidebarRow("shippingbox", "Packages")
      sidebarRow("clock.arrow.circlepath", "Activity", badge: "1")
      sidebarRow("square.stack.3d.up", "Sources")
      Spacer()

      if direction == .operationsDesk {
        VStack(alignment: .leading, spacing: 7) {
          Label("9 of 11 sources current", systemImage: "dot.radiowaves.left.and.right")
          Label("Updated 2 min ago", systemImage: "clock")
        }
        .font(.system(size: 11, weight: .medium))
        .foregroundStyle(palette.secondaryText)
        .padding(.horizontal, 10)
      } else {
        Label("Environment current", systemImage: "checkmark.seal")
          .font(.system(size: 12, weight: .medium))
          .foregroundStyle(palette.secondaryText)
          .padding(.horizontal, 10)
      }
    }
    .padding(.horizontal, 12)
    .padding(.vertical, 18)
    .frame(width: direction == .operationsDesk ? 205 : 220)
    .background(palette.sidebar)
  }

  private var brandLockup: some View {
    HStack(spacing: 10) {
      ZStack {
        RoundedRectangle(cornerRadius: 9, style: .continuous).fill(direction.accent.gradient)
        Image(systemName: "helm")
          .font(.system(size: 17, weight: .bold))
          .foregroundStyle(.white)
      }
      .frame(width: 35, height: 35)

      VStack(alignment: .leading, spacing: 1) {
        Text("HELM")
          .font(.system(size: 14, weight: .bold, design: .rounded))
          .tracking(1.6)
        Text(direction.name)
          .font(.system(size: 10, weight: .medium))
          .foregroundStyle(palette.secondaryText)
      }
    }
    .padding(.horizontal, 8)
  }

  private func sidebarRow(
    _ symbol: String,
    _ label: String,
    badge: String? = nil,
    selected: Bool = false
  ) -> some View {
    HStack(spacing: 10) {
      Image(systemName: symbol).font(.system(size: 14, weight: .medium)).frame(width: 18)
      Text(label).font(.system(size: 13, weight: selected ? .semibold : .regular))
      Spacer()
      if let badge {
        Text(badge)
          .font(.system(size: 11, weight: .semibold, design: .rounded))
          .padding(.horizontal, 7)
          .padding(.vertical, 2)
          .background(palette.raisedSurface, in: Capsule())
      }
    }
    .foregroundStyle(selected ? palette.primaryText : palette.secondaryText)
    .padding(.horizontal, 10)
    .frame(height: direction == .operationsDesk ? 34 : 38)
    .background {
      if selected {
        RoundedRectangle(cornerRadius: 7, style: .continuous)
          .fill(direction.accent.opacity(colorScheme == .dark ? 0.24 : 0.16))
      }
    }
  }

  @ViewBuilder
  private var mainContent: some View {
    switch direction {
    case .quietNative: quietNativeContent
    case .operationsDesk: operationsDeskContent
    case .guidedClarity: guidedClarityContent
    }
  }

  private var quietNativeContent: some View {
    VStack(alignment: .leading, spacing: 22) {
      pageHeader(
        eyebrow: "HEALTH",
        title: "Attention required",
        subtitle: "Two items need your review. Helm has not changed anything."
      )
      HStack(spacing: 12) {
        metric("12", "Updates ready", "arrow.down.circle")
        metric("9 / 11", "Sources current", "dot.radiowaves.left.and.right")
        metric("1", "Task running", "clock.arrow.circlepath")
      }
      sectionLabel("Needs attention")
      VStack(spacing: 0) {
        findingRow(
          .warning, "npm verification failed", "Review the package result before retrying",
          "2 min ago")
        Divider().overlay(palette.line).padding(.leading, 46)
        findingRow(.blocked, "Homebrew permission denied", "No packages were changed", "7 min ago")
      }
      .proposalSurface(palette)

      sectionLabel("Environment")
      HStack(spacing: 14) {
        Label("214 packages organized", systemImage: "shippingbox")
        Divider().frame(height: 16)
        Label("Current 2 min ago", systemImage: "checkmark.circle")
        Spacer()
        Text("View coverage").foregroundStyle(direction.accent)
      }
      .font(.system(size: 12, weight: .medium))
      .foregroundStyle(palette.secondaryText)
      .padding(16)
      .proposalSurface(palette)
      Spacer()
    }
    .padding(28)
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    .background(palette.canvas)
  }

  private var operationsDeskContent: some View {
    VStack(alignment: .leading, spacing: 0) {
      HStack(alignment: .center, spacing: 18) {
        VStack(alignment: .leading, spacing: 3) {
          Text("SYSTEM HEALTH")
            .font(.system(size: 10, weight: .bold, design: .monospaced))
            .tracking(1.2)
            .foregroundStyle(direction.accent)
          Text("2 findings require review")
            .font(.system(size: 23, weight: .semibold))
        }
        Spacer()
        compactMetric("12", "UPDATES")
        compactMetric("9/11", "CURRENT")
        compactMetric("1", "ACTIVE")
      }
      .padding(.horizontal, 24)
      .frame(height: 88)
      .background(palette.surface)

      Divider().overlay(palette.line)
      HStack(spacing: 0) {
        Text("STATUS").frame(width: 82, alignment: .leading)
        Text("FINDING").frame(maxWidth: .infinity, alignment: .leading)
        Text("SOURCE").frame(width: 105, alignment: .leading)
        Text("AGE").frame(width: 66, alignment: .trailing)
      }
      .font(.system(size: 10, weight: .bold, design: .monospaced))
      .foregroundStyle(palette.secondaryText)
      .padding(.horizontal, 20)
      .frame(height: 35)

      Divider().overlay(palette.line)
      deskRow("REVIEW", "npm verification failed", "npm", "2m", tone: direction.warmAccent)
      Divider().overlay(palette.line)
      deskRow("BLOCKED", "Homebrew permission denied", "Homebrew", "7m", tone: Color.red)
      Divider().overlay(palette.line)
      deskRow("NOTICE", "Duplicate rustup installations", "rustup", "1h", tone: direction.accent)

      HStack {
        Text("Coverage").font(.system(size: 11, weight: .bold, design: .monospaced))
        Text("9 current")
        Text("2 cached")
        Text("0 unavailable")
        Spacer()
        Label("Refresh in progress", systemImage: "arrow.triangle.2.circlepath")
          .foregroundStyle(direction.accent)
      }
      .font(.system(size: 11, weight: .medium, design: .monospaced))
      .foregroundStyle(palette.secondaryText)
      .padding(.horizontal, 20)
      .frame(height: 46)
      .background(palette.surface)
      Spacer()
    }
    .background(palette.canvas)
  }

  private var guidedClarityContent: some View {
    VStack(alignment: .leading, spacing: 20) {
      VStack(alignment: .leading, spacing: 10) {
        HStack {
          Label("Your environment", systemImage: "sparkles")
            .font(.system(size: 11, weight: .bold))
            .tracking(0.7)
            .foregroundStyle(direction.accent)
          Spacer()
          Text("Updated 2 min ago")
            .font(.system(size: 11))
            .foregroundStyle(palette.secondaryText)
        }
        Text("Your Mac is mapped.\nTwo things need a closer look.")
          .font(.system(size: 28, weight: .semibold, design: .rounded))
          .fixedSize(horizontal: false, vertical: true)
        Text(
          "Helm found 214 packages across 11 sources. Nothing changes until you review and approve it."
        )
        .font(.system(size: 13))
        .foregroundStyle(palette.secondaryText)
        .frame(maxWidth: 540, alignment: .leading)
        HStack(spacing: 10) {
          mockButton("Review findings", prominent: true)
          mockButton("View all updates", prominent: false)
        }
        .padding(.top, 2)
      }
      .padding(24)
      .background(
        LinearGradient(
          colors: [
            direction.accent.opacity(colorScheme == .dark ? 0.19 : 0.13),
            direction.warmAccent.opacity(0.07),
          ],
          startPoint: .topLeading,
          endPoint: .bottomTrailing
        ),
        in: RoundedRectangle(cornerRadius: 16, style: .continuous)
      )
      .overlay {
        RoundedRectangle(cornerRadius: 16, style: .continuous)
          .stroke(direction.accent.opacity(0.20), lineWidth: 1)
      }

      sectionLabel("What needs attention")
      HStack(spacing: 12) {
        guidedFinding(
          "npm", "Verification failed", "Review details before retrying",
          "exclamationmark.triangle.fill", direction.warmAccent)
        guidedFinding(
          "Homebrew", "Permission denied", "No packages were changed", "lock.fill",
          Color.red.opacity(0.82))
      }
      sectionLabel("At a glance")
      HStack(spacing: 12) {
        metric("12", "Updates ready", "arrow.down.circle")
        metric("9 / 11", "Sources current", "dot.radiowaves.left.and.right")
        metric("1", "Task running", "clock.arrow.circlepath")
      }
      Spacer()
    }
    .padding(26)
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    .background(palette.canvas)
  }

  private var inspector: some View {
    VStack(alignment: .leading, spacing: 0) {
      HStack {
        Text(direction == .operationsDesk ? "FINDING DETAIL" : "Finding")
          .font(
            direction == .operationsDesk
              ? .system(size: 11, weight: .bold, design: .monospaced)
              : .system(size: 13, weight: .semibold)
          )
          .tracking(direction == .operationsDesk ? 0.8 : 0)
        Spacer()
        Image(systemName: "xmark")
          .font(.system(size: 11, weight: .semibold))
          .foregroundStyle(palette.secondaryText)
      }
      .padding(.horizontal, 18)
      .frame(height: 48)
      Divider().overlay(palette.line)

      VStack(alignment: .leading, spacing: direction == .operationsDesk ? 16 : 20) {
        ZStack {
          RoundedRectangle(cornerRadius: 10, style: .continuous)
            .fill(direction.warmAccent.opacity(0.14))
          Image(systemName: "exclamationmark.triangle.fill")
            .font(.system(size: 20))
            .foregroundStyle(direction.warmAccent)
        }
        .frame(width: 42, height: 42)
        VStack(alignment: .leading, spacing: 5) {
          Text("npm verification failed").font(.system(size: 16, weight: .semibold))
          Text("Needs review · 2 min ago")
            .font(.system(size: 11, weight: .medium))
            .foregroundStyle(palette.secondaryText)
        }
        inspectorSection(
          "What happened",
          "npm returned package data that Helm could not verify. No package changes were made.")
        inspectorSection(
          "Recommended next step",
          "Review the command output, then retry this source when you are ready.")
        HStack(spacing: 8) {
          mockButton("Retry", prominent: true)
          mockButton("Diagnostics", prominent: false)
        }
        Divider().overlay(palette.line)
        Label("No rollback is needed", systemImage: "checkmark.shield")
          .font(.system(size: 11, weight: .medium))
          .foregroundStyle(palette.secondaryText)
        Spacer()
      }
      .padding(18)
    }
    .frame(width: direction == .operationsDesk ? 275 : 290)
    .background(palette.chrome)
  }

  private func pageHeader(eyebrow: String, title: String, subtitle: String) -> some View {
    VStack(alignment: .leading, spacing: 6) {
      Text(eyebrow)
        .font(.system(size: 10, weight: .bold))
        .tracking(1.1)
        .foregroundStyle(direction.accent)
      Text(title).font(.system(size: 28, weight: .semibold))
      Text(subtitle).font(.system(size: 13)).foregroundStyle(palette.secondaryText)
    }
  }

  private func sectionLabel(_ title: String) -> some View {
    Text(title).font(.system(size: 12, weight: .semibold)).foregroundStyle(palette.secondaryText)
  }

  private func metric(_ value: String, _ label: String, _ symbol: String) -> some View {
    VStack(alignment: .leading, spacing: 10) {
      HStack {
        Image(systemName: symbol).foregroundStyle(direction.accent)
        Spacer()
        Image(systemName: "chevron.right")
          .font(.system(size: 9, weight: .bold))
          .foregroundStyle(palette.secondaryText.opacity(0.65))
      }
      Text(value).font(.system(size: 23, weight: .semibold, design: .rounded))
      Text(label).font(.system(size: 11, weight: .medium)).foregroundStyle(palette.secondaryText)
    }
    .padding(15)
    .frame(maxWidth: .infinity, alignment: .leading)
    .proposalSurface(palette)
  }

  private func compactMetric(_ value: String, _ label: String) -> some View {
    VStack(alignment: .trailing, spacing: 2) {
      Text(value).font(.system(size: 18, weight: .semibold, design: .monospaced))
      Text(label)
        .font(.system(size: 9, weight: .bold, design: .monospaced))
        .tracking(0.5)
        .foregroundStyle(palette.secondaryText)
    }
    .frame(width: 58)
  }

  private enum FindingTone {
    case warning
    case blocked

    var symbol: String {
      switch self {
      case .warning: "exclamationmark.triangle.fill"
      case .blocked: "lock.fill"
      }
    }
  }

  private func findingRow(_ tone: FindingTone, _ title: String, _ detail: String, _ age: String)
    -> some View
  {
    HStack(spacing: 13) {
      Image(systemName: tone.symbol)
        .font(.system(size: 14))
        .foregroundStyle(tone == .warning ? direction.warmAccent : Color.red.opacity(0.85))
        .frame(width: 20)
      VStack(alignment: .leading, spacing: 3) {
        Text(title).font(.system(size: 13, weight: .medium))
        Text(detail).font(.system(size: 11)).foregroundStyle(palette.secondaryText)
      }
      Spacer()
      Text(age).font(.system(size: 10)).foregroundStyle(palette.secondaryText)
      Image(systemName: "chevron.right")
        .font(.system(size: 9, weight: .bold))
        .foregroundStyle(palette.secondaryText)
    }
    .padding(.horizontal, 14)
    .frame(height: 62)
  }

  private func deskRow(
    _ status: String, _ finding: String, _ source: String, _ age: String, tone: Color
  ) -> some View {
    HStack(spacing: 0) {
      HStack(spacing: 6) {
        Circle().fill(tone).frame(width: 6, height: 6)
        Text(status)
      }
      .font(.system(size: 9, weight: .bold, design: .monospaced))
      .foregroundStyle(tone)
      .frame(width: 82, alignment: .leading)
      Text(finding).font(.system(size: 12, weight: .medium)).frame(
        maxWidth: .infinity, alignment: .leading)
      Text(source)
        .font(.system(size: 11, design: .monospaced))
        .foregroundStyle(palette.secondaryText)
        .frame(width: 105, alignment: .leading)
      Text(age)
        .font(.system(size: 11, design: .monospaced))
        .foregroundStyle(palette.secondaryText)
        .frame(width: 66, alignment: .trailing)
    }
    .padding(.horizontal, 20)
    .frame(height: 52)
  }

  private func guidedFinding(
    _ source: String, _ title: String, _ detail: String, _ symbol: String, _ tone: Color
  ) -> some View {
    VStack(alignment: .leading, spacing: 10) {
      HStack {
        Image(systemName: symbol).foregroundStyle(tone)
        Spacer()
        Image(systemName: "arrow.up.right")
          .font(.system(size: 10, weight: .bold))
          .foregroundStyle(palette.secondaryText)
      }
      Text(source.uppercased())
        .font(.system(size: 9, weight: .bold))
        .tracking(0.8)
        .foregroundStyle(palette.secondaryText)
      Text(title).font(.system(size: 14, weight: .semibold))
      Text(detail).font(.system(size: 11)).foregroundStyle(palette.secondaryText)
    }
    .padding(16)
    .frame(maxWidth: .infinity, minHeight: 132, alignment: .topLeading)
    .proposalSurface(palette)
  }

  private func mockButton(_ title: String, prominent: Bool) -> some View {
    Text(title)
      .font(.system(size: 12, weight: .semibold))
      .foregroundStyle(prominent ? Color.white : palette.primaryText)
      .padding(.horizontal, 15)
      .frame(height: 32)
      .background(
        prominent ? direction.accent : palette.raisedSurface,
        in: RoundedRectangle(cornerRadius: 7, style: .continuous)
      )
      .overlay {
        if !prominent {
          RoundedRectangle(cornerRadius: 7, style: .continuous)
            .stroke(palette.line, lineWidth: 1)
        }
      }
  }

  private func inspectorSection(_ title: String, _ body: String) -> some View {
    VStack(alignment: .leading, spacing: 6) {
      Text(title).font(.system(size: 11, weight: .semibold)).foregroundStyle(palette.secondaryText)
      Text(body)
        .font(.system(size: 12))
        .lineSpacing(3)
        .fixedSize(horizontal: false, vertical: true)
    }
  }
}

extension View {
  fileprivate func proposalSurface(_ palette: ProposalPalette) -> some View {
    background(palette.surface, in: RoundedRectangle(cornerRadius: 11, style: .continuous))
      .overlay {
        RoundedRectangle(cornerRadius: 11, style: .continuous)
          .stroke(palette.line, lineWidth: 1)
      }
  }
}
