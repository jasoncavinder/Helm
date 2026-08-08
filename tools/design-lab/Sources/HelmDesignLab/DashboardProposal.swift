import SwiftUI

struct DashboardProposal: View {
  let destination: DashboardDestination

  @Environment(\.colorScheme) private var colorScheme

  private var palette: NavigatorPalette {
    NavigatorPalette(scheme: colorScheme)
  }

  var body: some View {
    HStack(spacing: 0) {
      sidebar
      Divider().overlay(palette.line)
      VStack(spacing: 0) {
        toolbar
        Divider().overlay(palette.line)
        page
      }
      .background(palette.canvas)
    }
    .frame(width: 1280, height: 800)
    .foregroundStyle(palette.primaryText)
    .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
    .overlay {
      RoundedRectangle(cornerRadius: 18, style: .continuous)
        .stroke(palette.line, lineWidth: 1)
    }
    .shadow(color: .black.opacity(colorScheme == .dark ? 0.48 : 0.18), radius: 28, y: 14)
    .padding(34)
  }

  private var sidebar: some View {
    VStack(alignment: .leading, spacing: 0) {
      HStack(spacing: 8) {
        Circle().fill(Color(red: 1.0, green: 0.38, blue: 0.34)).frame(width: 12, height: 12)
        Circle().fill(Color(red: 1.0, green: 0.74, blue: 0.24)).frame(width: 12, height: 12)
        Circle().fill(Color(red: 0.16, green: 0.78, blue: 0.31)).frame(width: 12, height: 12)
      }
      .padding(.horizontal, 20)
      .frame(height: 52)

      HStack(spacing: 11) {
        ZStack {
          RoundedRectangle(cornerRadius: 11, style: .continuous)
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
        .frame(width: 42, height: 42)

        VStack(alignment: .leading, spacing: 1) {
          Text("HELM")
            .font(.system(size: 15, weight: .bold, design: .rounded))
            .tracking(1.8)
          Text("Software, on course")
            .font(.system(size: 10, weight: .medium))
            .foregroundStyle(palette.secondaryText)
        }
      }
      .padding(.horizontal, 18)
      .padding(.bottom, 18)

      HStack(spacing: 8) {
        Image(systemName: "magnifyingglass")
        Text("Find anything")
        Spacer()
        Text("⌘K")
          .font(.system(size: 10, weight: .semibold, design: .rounded))
      }
      .font(.system(size: 12))
      .foregroundStyle(palette.secondaryText)
      .padding(.horizontal, 11)
      .frame(height: 34)
      .background(palette.raisedSurface.opacity(0.72), in: RoundedRectangle(cornerRadius: 9))
      .overlay {
        RoundedRectangle(cornerRadius: 9).stroke(palette.line, lineWidth: 1)
      }
      .padding(.horizontal, 14)
      .padding(.bottom, 20)

      Text("WORKSPACE")
        .font(.system(size: 9, weight: .bold))
        .tracking(1.1)
        .foregroundStyle(palette.secondaryText)
        .padding(.horizontal, 24)
        .padding(.bottom, 7)

      ForEach(DashboardDestination.allCases, id: \.self) { item in
        sidebarRow(item)
      }

      Spacer()

      VStack(alignment: .leading, spacing: 9) {
        HStack {
          Label("Environment", systemImage: "point.3.connected.trianglepath.dotted")
            .font(.system(size: 12, weight: .semibold))
          Spacer()
          Image(systemName: "chevron.right")
            .font(.system(size: 9, weight: .bold))
            .foregroundStyle(palette.secondaryText)
        }
        HStack(spacing: 7) {
          Circle().fill(NavigatorPalette.seaGlass).frame(width: 7, height: 7)
          Text("9 of 11 sources current")
            .font(.system(size: 10, weight: .medium))
            .foregroundStyle(palette.secondaryText)
        }
      }
      .padding(13)
      .background(palette.raisedSurface.opacity(0.52), in: RoundedRectangle(cornerRadius: 11))
      .overlay {
        RoundedRectangle(cornerRadius: 11).stroke(palette.line, lineWidth: 1)
      }
      .padding(.horizontal, 14)

      HStack {
        Label("Updated 2 min ago", systemImage: "clock")
        Spacer()
        Image(systemName: "gearshape")
      }
      .font(.system(size: 10, weight: .medium))
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

  private func sidebarRow(_ item: DashboardDestination) -> some View {
    let selected = item == destination
    return HStack(spacing: 11) {
      Image(systemName: item.symbol)
        .font(.system(size: 14, weight: .medium))
        .frame(width: 18)
      Text(item.rawValue)
        .font(.system(size: 13, weight: selected ? .semibold : .regular))
      Spacer()
      if item == .plan {
        Text("12")
          .font(.system(size: 10, weight: .semibold, design: .rounded))
          .padding(.horizontal, 7)
          .padding(.vertical, 2)
          .background(palette.raisedSurface, in: Capsule())
      } else if item == .activity {
        Circle().fill(NavigatorPalette.seaGlass).frame(width: 7, height: 7)
      }
    }
    .foregroundStyle(selected ? palette.primaryText : palette.secondaryText)
    .padding(.horizontal, 11)
    .frame(height: 40)
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
        Image(systemName: "chevron.left")
          .foregroundStyle(palette.secondaryText)
          .frame(width: 31, height: 30)
        Divider().frame(height: 17)
        Image(systemName: "chevron.right")
          .foregroundStyle(palette.secondaryText.opacity(0.35))
          .frame(width: 31, height: 30)
      }
      .background(palette.raisedSurface, in: Capsule())
      .overlay { Capsule().stroke(palette.line, lineWidth: 1) }

      Text(destination.rawValue)
        .font(.system(size: 17, weight: .semibold))

      Spacer()

      ButtonLike(symbol: "arrow.clockwise", palette: palette)
      ButtonLike(symbol: "ellipsis", palette: palette)
    }
    .padding(.horizontal, 20)
    .frame(height: 62)
    .background(palette.chrome)
  }

  @ViewBuilder
  private var page: some View {
    switch destination {
    case .dashboard: overviewPage
    case .plan: planPage
    case .library: libraryPage
    case .activity: activityPlaceholder
    }
  }

  private var overviewPage: some View {
    VStack(alignment: .leading, spacing: 18) {
      HStack(spacing: 28) {
        StatusCompass(palette: palette, status: .attention)
          .frame(width: 178, height: 178)

        VStack(alignment: .leading, spacing: 9) {
          Text("YOUR SOFTWARE ENVIRONMENT")
            .font(.system(size: 10, weight: .bold))
            .tracking(1.2)
            .foregroundStyle(NavigatorPalette.helmBlue)
          Text("4 updates are ready.")
            .font(.system(size: 32, weight: .semibold, design: .rounded))
          Text("Everything else is on course.")
            .font(.system(size: 22, weight: .medium, design: .rounded))
            .foregroundStyle(palette.secondaryText)
          Text(
            "Helm mapped 214 apps, tools, and packages. Nothing changes until you approve a plan."
          )
          .font(.system(size: 12))
          .foregroundStyle(palette.secondaryText)
          .padding(.top, 2)
          HStack(spacing: 9) {
            primaryButton("Review Plan", symbol: "arrow.right")
            secondaryButton("Check Again", symbol: "arrow.clockwise")
          }
          .padding(.top, 5)
        }
        Spacer()
      }
      .padding(.horizontal, 30)
      .frame(height: 226)
      .background {
        ZStack {
          LinearGradient(
            colors: [
              NavigatorPalette.helmBlue.opacity(colorScheme == .dark ? 0.18 : 0.11),
              NavigatorPalette.seaGlass.opacity(colorScheme == .dark ? 0.09 : 0.07),
              NavigatorPalette.attention.opacity(0.035),
            ],
            startPoint: .topLeading,
            endPoint: .bottomTrailing
          )
          HorizonLines(color: NavigatorPalette.horizon.opacity(colorScheme == .dark ? 0.13 : 0.09))
        }
      }
      .clipShape(RoundedRectangle(cornerRadius: 20, style: .continuous))
      .overlay {
        RoundedRectangle(cornerRadius: 20, style: .continuous)
          .stroke(NavigatorPalette.helmBlue.opacity(0.16), lineWidth: 1)
      }

      Text("Your environment")
        .font(.system(size: 13, weight: .semibold))
        .foregroundStyle(palette.secondaryText)

      EnvironmentRoute(palette: palette)
        .frame(height: 126)

      HStack(alignment: .top, spacing: 18) {
        VStack(alignment: .leading, spacing: 0) {
          HStack {
            Text("Needs you")
              .font(.system(size: 13, weight: .semibold))
            Spacer()
            Text("2 items")
              .font(.system(size: 11, weight: .medium))
              .foregroundStyle(palette.secondaryText)
          }
          .padding(.horizontal, 15)
          .frame(height: 39)
          Divider().overlay(palette.line)
          attentionRow(
            symbol: "exclamationmark.triangle.fill",
            title: "npm could not verify its results",
            detail: "No packages changed · Review before retrying",
            tone: NavigatorPalette.attention
          )
          Divider().overlay(palette.line).padding(.leading, 51)
          attentionRow(
            symbol: "square.stack.3d.up.badge.a",
            title: "Two Node versions are intentionally installed",
            detail: "Node 24 and Node 26 · Shims grouped correctly",
            tone: NavigatorPalette.helmBlue
          )
        }
        .frame(maxWidth: .infinity)
        .navigatorSurface(palette)

        VStack(alignment: .leading, spacing: 9) {
          HStack {
            Image(systemName: "waveform.path.ecg")
              .foregroundStyle(NavigatorPalette.seaGlass)
            Text("Current activity")
              .font(.system(size: 12, weight: .semibold))
            Spacer()
            Image(systemName: "chevron.right")
              .font(.system(size: 9, weight: .bold))
              .foregroundStyle(palette.secondaryText)
          }
          Text("Refreshing application updates")
            .font(.system(size: 12, weight: .medium))
          ProposalProgressTrack(progress: 0.67, palette: palette)
          Text("6 of 9 sources checked")
            .font(.system(size: 10))
            .foregroundStyle(palette.secondaryText)
        }
        .padding(15)
        .frame(width: 260, height: 137, alignment: .topLeading)
        .navigatorSurface(palette)
      }
    }
    .padding(24)
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
  }

  private var planPage: some View {
    HStack(alignment: .top, spacing: 28) {
      VStack(alignment: .leading, spacing: 18) {
        VStack(alignment: .leading, spacing: 5) {
          Text("A safe route through 12 updates")
            .font(.system(size: 29, weight: .semibold, design: .rounded))
          Text(
            "Helm orders work by authority, verifies every stage, and stops before dependent work if needed."
          )
          .font(.system(size: 12))
          .foregroundStyle(palette.secondaryText)
        }

        VStack(spacing: 0) {
          planStage(
            number: "1",
            title: "Toolchains first",
            detail: "mise · rustup",
            count: "2 updates",
            status: "Ready",
            tone: NavigatorPalette.seaGlass,
            isLast: false
          )
          planStage(
            number: "2",
            title: "Applications",
            detail: "Homebrew Casks · Sparkle",
            count: "3 updates",
            status: "Authorization",
            tone: NavigatorPalette.attention,
            isLast: false
          )
          planStage(
            number: "3",
            title: "Packages",
            detail: "npm · Cargo · RubyGems",
            count: "7 updates",
            status: "Ready",
            tone: NavigatorPalette.seaGlass,
            isLast: false
          )
          planStage(
            number: "—",
            title: "macOS update excluded",
            detail: "Guarded system updates always run separately",
            count: "1 available",
            status: "Excluded",
            tone: palette.secondaryText,
            isLast: true
          )
        }
        .navigatorSurface(palette, cornerRadius: 16)

        Label(
          "Pinned packages and unsupported actions are omitted before confirmation.",
          systemImage: "checkmark.shield"
        )
        .font(.system(size: 11, weight: .medium))
        .foregroundStyle(palette.secondaryText)
      }
      .frame(maxWidth: .infinity, alignment: .leading)

      VStack(alignment: .leading, spacing: 18) {
        Text("Plan summary")
          .font(.system(size: 16, weight: .semibold))
        summaryLine("Selected", value: "12")
        summaryLine("Download", value: "428 MB")
        summaryLine("Authorization", value: "1 stage")
        summaryLine("Restart", value: "Not expected")
        summaryLine("Pinned", value: "1 excluded")
        Divider().overlay(palette.line)
        VStack(alignment: .leading, spacing: 6) {
          Label("No automatic rollback", systemImage: "arrow.uturn.backward.circle")
            .font(.system(size: 11, weight: .semibold))
          Text(
            "Helm verifies each stage and preserves receipts, but package managers define recovery limits."
          )
          .font(.system(size: 10))
          .foregroundStyle(palette.secondaryText)
          .fixedSize(horizontal: false, vertical: true)
        }
        primaryButton("Review & Run", symbol: "arrow.right")
          .frame(maxWidth: .infinity, alignment: .trailing)
      }
      .padding(20)
      .frame(width: 300, alignment: .topLeading)
      .navigatorSurface(palette, cornerRadius: 16)
    }
    .padding(28)
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
  }

  private var libraryPage: some View {
    VStack(alignment: .leading, spacing: 20) {
      VStack(alignment: .leading, spacing: 6) {
        Text("Everything Helm knows about")
          .font(.system(size: 29, weight: .semibold, design: .rounded))
        Text("Find an app, tool, or package without needing to know which manager owns it.")
          .font(.system(size: 12))
          .foregroundStyle(palette.secondaryText)
      }

      HStack(spacing: 10) {
        Image(systemName: "magnifyingglass")
          .font(.system(size: 16, weight: .medium))
          .foregroundStyle(NavigatorPalette.helmBlue)
        Text("ripgrep")
          .font(.system(size: 15))
        Spacer()
        Text("Local results")
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(palette.secondaryText)
          .padding(.horizontal, 8)
          .padding(.vertical, 4)
          .background(palette.surface, in: Capsule())
        Text("⌘K")
          .font(.system(size: 10, weight: .semibold, design: .rounded))
          .foregroundStyle(palette.secondaryText)
      }
      .padding(.horizontal, 16)
      .frame(height: 48)
      .background(palette.raisedSurface, in: RoundedRectangle(cornerRadius: 12))
      .overlay { RoundedRectangle(cornerRadius: 12).stroke(palette.line, lineWidth: 1) }

      HStack(spacing: 8) {
        scopePill("All", selected: true)
        scopePill("Installed", selected: false)
        scopePill("Updates", selected: false)
        scopePill("Available", selected: false)
        Spacer()
        Label("3 matches", systemImage: "line.3.horizontal.decrease")
          .font(.system(size: 11, weight: .medium))
          .foregroundStyle(palette.secondaryText)
      }

      VStack(spacing: 0) {
        libraryRow(
          symbol: "shippingbox.fill",
          name: "ripgrep",
          source: "Homebrew",
          detail: "Recommended · Native Apple silicon bottle",
          version: "14.1.1",
          action: "Install",
          selected: true
        )
        Divider().overlay(palette.line).padding(.leading, 66)
        libraryRow(
          symbol: "shippingbox",
          name: "ripgrep",
          source: "Cargo",
          detail: "Build from source · Rust toolchain required",
          version: "14.1.1",
          action: "Choose",
          selected: false
        )
        Divider().overlay(palette.line).padding(.leading, 66)
        libraryRow(
          symbol: "externaldrive.connected.to.line.below",
          name: "ripgrep-all",
          source: "Homebrew",
          detail: "Related package · Searches PDFs and archives",
          version: "0.10.6",
          action: "View",
          selected: false
        )
      }
      .navigatorSurface(palette, cornerRadius: 16)

      HStack(spacing: 7) {
        Image(systemName: "network")
        Text("Remote catalog search is still running. Local and cached results remain usable.")
      }
      .font(.system(size: 10, weight: .medium))
      .foregroundStyle(palette.secondaryText)
    }
    .padding(28)
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
  }

  private var activityPlaceholder: some View {
    Text("Activity")
      .font(.title)
      .frame(maxWidth: .infinity, maxHeight: .infinity)
  }

  private func attentionRow(symbol: String, title: String, detail: String, tone: Color) -> some View
  {
    HStack(spacing: 12) {
      ZStack {
        RoundedRectangle(cornerRadius: 8).fill(tone.opacity(0.14))
        Image(systemName: symbol).font(.system(size: 13)).foregroundStyle(tone)
      }
      .frame(width: 31, height: 31)
      VStack(alignment: .leading, spacing: 2) {
        Text(title).font(.system(size: 11, weight: .semibold))
        Text(detail).font(.system(size: 9)).foregroundStyle(palette.secondaryText)
      }
      Spacer()
      Image(systemName: "chevron.right")
        .font(.system(size: 9, weight: .bold))
        .foregroundStyle(palette.secondaryText)
    }
    .padding(.horizontal, 14)
    .frame(height: 48)
  }

  private func planStage(
    number: String,
    title: String,
    detail: String,
    count: String,
    status: String,
    tone: Color,
    isLast: Bool
  ) -> some View {
    HStack(spacing: 14) {
      VStack(spacing: 0) {
        ZStack {
          Circle().fill(tone.opacity(0.15))
          Text(number).font(.system(size: 11, weight: .bold, design: .rounded)).foregroundStyle(
            tone)
        }
        .frame(width: 31, height: 31)
        if !isLast {
          Rectangle().fill(palette.line).frame(width: 1, height: 28)
        }
      }
      .frame(width: 31, height: 59, alignment: .top)
      VStack(alignment: .leading, spacing: 3) {
        Text(title).font(.system(size: 13, weight: .semibold))
        Text(detail).font(.system(size: 10)).foregroundStyle(palette.secondaryText)
      }
      Spacer()
      VStack(alignment: .trailing, spacing: 3) {
        Text(count).font(.system(size: 11, weight: .semibold))
        Text(status).font(.system(size: 9, weight: .medium)).foregroundStyle(tone)
      }
    }
    .padding(.horizontal, 17)
    .frame(height: 70)
  }

  private func summaryLine(_ label: String, value: String) -> some View {
    HStack {
      Text(label).foregroundStyle(palette.secondaryText)
      Spacer()
      Text(value).fontWeight(.medium)
    }
    .font(.system(size: 11))
  }

  private func libraryRow(
    symbol: String,
    name: String,
    source: String,
    detail: String,
    version: String,
    action: String,
    selected: Bool
  ) -> some View {
    HStack(spacing: 14) {
      ZStack {
        RoundedRectangle(cornerRadius: 10)
          .fill(selected ? NavigatorPalette.helmBlue.opacity(0.16) : palette.surface)
        Image(systemName: symbol)
          .font(.system(size: 17, weight: .medium))
          .foregroundStyle(selected ? NavigatorPalette.helmBlue : palette.secondaryText)
      }
      .frame(width: 39, height: 39)
      VStack(alignment: .leading, spacing: 3) {
        HStack(spacing: 7) {
          Text(name).font(.system(size: 13, weight: .semibold))
          Text(source)
            .font(.system(size: 9, weight: .semibold))
            .foregroundStyle(palette.secondaryText)
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(palette.surface, in: Capsule())
        }
        Text(detail).font(.system(size: 10)).foregroundStyle(palette.secondaryText)
      }
      Spacer()
      Text(version).font(.system(size: 10, design: .monospaced)).foregroundStyle(
        palette.secondaryText)
      secondaryButton(action, symbol: selected ? "arrow.down" : "chevron.right")
    }
    .padding(.horizontal, 14)
    .frame(height: 72)
    .background(
      selected ? NavigatorPalette.helmBlue.opacity(colorScheme == .dark ? 0.12 : 0.055) : .clear)
  }

  private func scopePill(_ title: String, selected: Bool) -> some View {
    Text(title)
      .font(.system(size: 10, weight: selected ? .semibold : .medium))
      .foregroundStyle(selected ? Color.white : palette.secondaryText)
      .padding(.horizontal, 11)
      .frame(height: 26)
      .background(selected ? NavigatorPalette.helmBlue : palette.surface, in: Capsule())
  }

  private func primaryButton(_ title: String, symbol: String) -> some View {
    HStack(spacing: 7) {
      Text(title)
      Image(systemName: symbol).font(.system(size: 9, weight: .bold))
    }
    .font(.system(size: 11, weight: .semibold))
    .foregroundStyle(.white)
    .padding(.horizontal, 14)
    .frame(height: 32)
    .background(
      LinearGradient(
        colors: [NavigatorPalette.helmBlue, NavigatorPalette.seaGlass],
        startPoint: .leading,
        endPoint: .trailing
      ),
      in: RoundedRectangle(cornerRadius: 8)
    )
  }

  private func secondaryButton(_ title: String, symbol: String) -> some View {
    HStack(spacing: 7) {
      Text(title)
      Image(systemName: symbol).font(.system(size: 9, weight: .bold))
    }
    .font(.system(size: 11, weight: .semibold))
    .padding(.horizontal, 13)
    .frame(height: 32)
    .background(palette.raisedSurface, in: RoundedRectangle(cornerRadius: 8))
    .overlay { RoundedRectangle(cornerRadius: 8).stroke(palette.line, lineWidth: 1) }
  }
}

private struct ButtonLike: View {
  let symbol: String
  let palette: NavigatorPalette

  var body: some View {
    Image(systemName: symbol)
      .font(.system(size: 13, weight: .medium))
      .frame(width: 30, height: 30)
      .background(palette.raisedSurface, in: Circle())
      .overlay { Circle().stroke(palette.line, lineWidth: 1) }
  }
}

private struct StatusCompass: View {
  enum Status {
    case attention
  }

  let palette: NavigatorPalette
  let status: Status

  var body: some View {
    ZStack {
      Circle()
        .stroke(
          LinearGradient(
            colors: [
              NavigatorPalette.helmBlue.opacity(0.16), NavigatorPalette.seaGlass.opacity(0.42),
            ],
            startPoint: .topLeading,
            endPoint: .bottomTrailing
          ),
          lineWidth: 13
        )
      Circle()
        .trim(from: 0.03, to: 0.77)
        .stroke(
          LinearGradient(
            colors: [NavigatorPalette.seaGlass, NavigatorPalette.helmBlue],
            startPoint: .topLeading,
            endPoint: .bottomTrailing
          ),
          style: StrokeStyle(lineWidth: 13, lineCap: .round)
        )
        .rotationEffect(.degrees(-90))
      Circle().fill(palette.raisedSurface).padding(25)
      ForEach(0..<8, id: \.self) { index in
        Capsule()
          .fill(palette.secondaryText.opacity(0.34))
          .frame(width: 2, height: 9)
          .offset(y: -59)
          .rotationEffect(.degrees(Double(index) * 45))
      }
      VStack(spacing: 5) {
        Image(systemName: "helm")
          .font(.system(size: 38, weight: .medium))
          .foregroundStyle(NavigatorPalette.helmBlue)
        HStack(spacing: 5) {
          Circle().fill(NavigatorPalette.attention).frame(width: 6, height: 6)
          Text("4 READY")
            .font(.system(size: 9, weight: .bold, design: .rounded))
            .tracking(0.7)
            .foregroundStyle(palette.secondaryText)
        }
      }
    }
  }
}

private struct EnvironmentRoute: View {
  let palette: NavigatorPalette

  private let stages = [
    ("desktopcomputer", "System", "macOS guarded", NavigatorPalette.seaGlass),
    ("hammer", "Toolchains", "4 managers", NavigatorPalette.seaGlass),
    ("app.dashed", "Applications", "2 updates", NavigatorPalette.attention),
    ("shippingbox", "Packages", "2 updates", NavigatorPalette.attention),
  ]

  var body: some View {
    HStack(spacing: 0) {
      ForEach(Array(stages.enumerated()), id: \.offset) { index, stage in
        routeNode(symbol: stage.0, title: stage.1, detail: stage.2, tone: stage.3)
        if index < stages.count - 1 {
          ZStack {
            Rectangle().fill(palette.line).frame(height: 2)
            Circle().fill(NavigatorPalette.seaGlass).frame(width: 5, height: 5)
          }
          .frame(maxWidth: .infinity)
          .offset(y: -20)
        }
      }
    }
    .padding(.horizontal, 22)
    .navigatorSurface(palette, cornerRadius: 16)
  }

  private func routeNode(symbol: String, title: String, detail: String, tone: Color) -> some View {
    VStack(spacing: 6) {
      ZStack {
        Circle().fill(tone.opacity(0.13))
        Image(systemName: symbol).font(.system(size: 15, weight: .medium)).foregroundStyle(tone)
      }
      .frame(width: 38, height: 38)
      Text(title).font(.system(size: 10, weight: .semibold))
      Text(detail).font(.system(size: 9)).foregroundStyle(palette.secondaryText)
    }
    .frame(width: 105)
  }
}

private struct HorizonLines: View {
  let color: Color

  var body: some View {
    GeometryReader { geometry in
      Path { path in
        path.move(to: CGPoint(x: geometry.size.width * 0.57, y: 0))
        path.addCurve(
          to: CGPoint(x: geometry.size.width, y: geometry.size.height * 0.36),
          control1: CGPoint(x: geometry.size.width * 0.74, y: geometry.size.height * 0.02),
          control2: CGPoint(x: geometry.size.width * 0.83, y: geometry.size.height * 0.34)
        )
        path.move(to: CGPoint(x: geometry.size.width * 0.68, y: geometry.size.height))
        path.addCurve(
          to: CGPoint(x: geometry.size.width, y: geometry.size.height * 0.67),
          control1: CGPoint(x: geometry.size.width * 0.79, y: geometry.size.height * 0.91),
          control2: CGPoint(x: geometry.size.width * 0.88, y: geometry.size.height * 0.65)
        )
      }
      .stroke(color, style: StrokeStyle(lineWidth: 2, dash: [5, 8]))
    }
  }
}
