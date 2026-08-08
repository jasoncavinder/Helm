import SwiftUI

struct BriefingDashboardProposal: View {
  @Environment(\.colorScheme) private var colorScheme

  private var palette: NavigatorPalette { NavigatorPalette(scheme: colorScheme) }

  var body: some View {
    VStack(spacing: 0) {
      titlebar
      Divider().overlay(palette.line)
      HStack(alignment: .top, spacing: 24) {
        brief
        coursePanel
      }
      .padding(28)
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

  private var titlebar: some View {
    HStack(spacing: 18) {
      HStack(spacing: 8) {
        Circle().fill(Color(red: 1, green: 0.38, blue: 0.34)).frame(width: 12, height: 12)
        Circle().fill(Color(red: 1, green: 0.74, blue: 0.24)).frame(width: 12, height: 12)
        Circle().fill(Color(red: 0.16, green: 0.78, blue: 0.31)).frame(width: 12, height: 12)
      }
      HStack(spacing: 9) {
        Image(systemName: "helm")
          .font(.system(size: 17, weight: .bold))
          .foregroundStyle(NavigatorPalette.helmBlue)
        Text("Helm").font(.system(size: 15, weight: .semibold))
      }

      Spacer()

      HStack(spacing: 4) {
        topDestination("Brief", selected: true)
        topDestination("Plan", selected: false)
        topDestination("Library", selected: false)
        topDestination("Activity", selected: false)
      }
      .padding(4)
      .background(palette.surface, in: Capsule())
      .overlay { Capsule().stroke(palette.line, lineWidth: 1) }

      Spacer()

      Image(systemName: "magnifyingglass")
      Image(systemName: "gearshape")
    }
    .font(.system(size: 13, weight: .medium))
    .foregroundStyle(palette.secondaryText)
    .padding(.horizontal, 20)
    .frame(height: 62)
    .background(palette.chrome)
  }

  private func topDestination(_ title: String, selected: Bool) -> some View {
    Text(title)
      .font(.system(size: 11, weight: selected ? .semibold : .medium))
      .foregroundStyle(selected ? palette.primaryText : palette.secondaryText)
      .padding(.horizontal, 17)
      .frame(height: 29)
      .background(selected ? palette.raisedSurface : .clear, in: Capsule())
      .shadow(color: selected ? .black.opacity(0.08) : .clear, radius: 3, y: 1)
  }

  private var brief: some View {
    VStack(alignment: .leading, spacing: 18) {
      VStack(alignment: .leading, spacing: 7) {
        Text("FRIDAY · SOFTWARE BRIEF")
          .font(.system(size: 10, weight: .bold))
          .tracking(1.35)
          .foregroundStyle(NavigatorPalette.helmBlue)
        Text("Your Mac has a clear route forward.")
          .font(.system(size: 34, weight: .semibold, design: .rounded))
        Text("Four updates are ready. Two findings deserve context, but nothing is blocked.")
          .font(.system(size: 13))
          .foregroundStyle(palette.secondaryText)
      }

      HStack(spacing: 19) {
        ZStack {
          RoundedRectangle(cornerRadius: 18, style: .continuous)
            .fill(
              LinearGradient(
                colors: [NavigatorPalette.helmBlue, NavigatorPalette.seaGlass],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
              )
            )
          VStack(alignment: .leading, spacing: 5) {
            Text("04").font(.system(size: 47, weight: .medium, design: .rounded))
            Text("UPDATES READY")
              .font(.system(size: 9, weight: .bold))
              .tracking(1.1)
          }
          .foregroundStyle(.white)
        }
        .frame(width: 150, height: 127)

        VStack(alignment: .leading, spacing: 10) {
          Text("A safe plan is already prepared")
            .font(.system(size: 17, weight: .semibold, design: .rounded))
          Text(
            "Toolchains first, then applications and packages. One authorization is expected; no restart is required."
          )
          .font(.system(size: 11))
          .foregroundStyle(palette.secondaryText)
          .fixedSize(horizontal: false, vertical: true)
          HStack(spacing: 8) {
            briefingButton("Review Plan", primary: true)
            briefingButton("Later", primary: false)
          }
        }
        Spacer()
      }
      .padding(18)
      .background {
        LinearGradient(
          colors: [
            NavigatorPalette.helmBlue.opacity(colorScheme == .dark ? 0.17 : 0.09),
            NavigatorPalette.seaGlass.opacity(colorScheme == .dark ? 0.10 : 0.05),
          ],
          startPoint: .topLeading,
          endPoint: .bottomTrailing
        )
      }
      .clipShape(RoundedRectangle(cornerRadius: 20, style: .continuous))
      .overlay {
        RoundedRectangle(cornerRadius: 20, style: .continuous)
          .stroke(NavigatorPalette.helmBlue.opacity(0.16), lineWidth: 1)
      }

      HStack {
        Text("Today’s brief").font(.system(size: 15, weight: .semibold))
        Spacer()
        Text("Updated 2 min ago")
          .font(.system(size: 10, weight: .medium))
          .foregroundStyle(palette.secondaryText)
      }

      VStack(spacing: 0) {
        briefingRow(
          time: "NOW", symbol: "exclamationmark.triangle.fill", tone: NavigatorPalette.attention,
          title: "npm could not verify its refresh",
          detail: "No packages changed · evidence preserved")
        Divider().overlay(palette.line).padding(.leading, 64)
        briefingRow(
          time: "NEXT", symbol: "square.stack.3d.up", tone: NavigatorPalette.helmBlue,
          title: "Node 24 and 26 are intentionally installed",
          detail: "Shims grouped · no action needed")
        Divider().overlay(palette.line).padding(.leading, 64)
        briefingRow(
          time: "LIVE", symbol: "waveform.path.ecg", tone: NavigatorPalette.seaGlass,
          title: "Application refresh is 67% complete", detail: "6 of 9 sources checked")
      }
      .navigatorSurface(palette, cornerRadius: 17)
    }
    .frame(maxWidth: .infinity, alignment: .topLeading)
  }

  private var coursePanel: some View {
    VStack(alignment: .leading, spacing: 18) {
      ZStack {
        Circle()
          .stroke(NavigatorPalette.seaGlass.opacity(0.18), lineWidth: 13)
        Circle()
          .trim(from: 0.02, to: 0.79)
          .stroke(
            LinearGradient(
              colors: [NavigatorPalette.helmBlue, NavigatorPalette.seaGlass],
              startPoint: .topLeading,
              endPoint: .bottomTrailing
            ),
            style: StrokeStyle(lineWidth: 13, lineCap: .round)
          )
          .rotationEffect(.degrees(-90))
        VStack(spacing: 5) {
          Image(systemName: "helm")
            .font(.system(size: 34, weight: .semibold))
            .foregroundStyle(NavigatorPalette.helmBlue)
          Text("ON COURSE")
            .font(.system(size: 9, weight: .bold))
            .tracking(1)
            .foregroundStyle(palette.secondaryText)
        }
      }
      .frame(width: 154, height: 154)
      .frame(maxWidth: .infinity)

      VStack(alignment: .leading, spacing: 3) {
        Text("214 items mapped").font(.system(size: 16, weight: .semibold))
        Text("Across 11 software sources")
          .font(.system(size: 10))
          .foregroundStyle(palette.secondaryText)
      }

      VStack(spacing: 12) {
        coverageLine("System", detail: "Guarded", tone: NavigatorPalette.seaGlass)
        coverageLine("Toolchains", detail: "Current", tone: NavigatorPalette.seaGlass)
        coverageLine("Applications", detail: "2 ready", tone: NavigatorPalette.attention)
        coverageLine("Packages", detail: "2 ready", tone: NavigatorPalette.attention)
      }

      Divider().overlay(palette.line)

      VStack(alignment: .leading, spacing: 8) {
        Label("Environment", systemImage: "point.3.connected.trianglepath.dotted")
          .font(.system(size: 12, weight: .semibold))
        Text("9 sources current · 2 cached")
          .font(.system(size: 10))
          .foregroundStyle(palette.secondaryText)
        Text("View coverage →")
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(NavigatorPalette.helmBlue)
      }
    }
    .padding(22)
    .frame(width: 292, alignment: .topLeading)
    .navigatorSurface(palette, cornerRadius: 20)
  }

  private func briefingRow(time: String, symbol: String, tone: Color, title: String, detail: String)
    -> some View
  {
    HStack(spacing: 14) {
      Text(time)
        .font(.system(size: 8, weight: .bold))
        .tracking(0.7)
        .foregroundStyle(palette.secondaryText)
        .frame(width: 38, alignment: .leading)
      ZStack {
        Circle().fill(tone.opacity(0.14))
        Image(systemName: symbol).font(.system(size: 12)).foregroundStyle(tone)
      }
      .frame(width: 31, height: 31)
      VStack(alignment: .leading, spacing: 3) {
        Text(title).font(.system(size: 12, weight: .semibold))
        Text(detail).font(.system(size: 9)).foregroundStyle(palette.secondaryText)
      }
      Spacer()
      Image(systemName: "chevron.right")
        .font(.system(size: 9, weight: .bold))
        .foregroundStyle(palette.secondaryText)
    }
    .padding(.horizontal, 15)
    .frame(height: 66)
  }

  private func coverageLine(_ title: String, detail: String, tone: Color) -> some View {
    HStack {
      Circle().fill(tone).frame(width: 7, height: 7)
      Text(title).font(.system(size: 11, weight: .medium))
      Spacer()
      Text(detail).font(.system(size: 10)).foregroundStyle(palette.secondaryText)
    }
  }

  private func briefingButton(_ title: String, primary: Bool) -> some View {
    Text(title)
      .font(.system(size: 10, weight: .semibold))
      .foregroundStyle(primary ? .white : palette.primaryText)
      .padding(.horizontal, 14)
      .frame(height: 31)
      .background(primary ? NavigatorPalette.helmBlue : palette.raisedSurface, in: Capsule())
      .overlay { Capsule().stroke(primary ? .clear : palette.line, lineWidth: 1) }
  }
}

struct BriefingPopoverProposal: View {
  @Environment(\.colorScheme) private var colorScheme

  private var palette: NavigatorPalette { NavigatorPalette(scheme: colorScheme) }

  var body: some View {
    VStack(spacing: 0) {
      HStack {
        Label("Helm", systemImage: "helm").font(.system(size: 13, weight: .semibold))
        Spacer()
        Text("Updated 2 min ago")
          .font(.system(size: 9, weight: .medium))
          .foregroundStyle(palette.secondaryText)
      }
      .padding(.horizontal, 18)
      .frame(height: 48)

      VStack(alignment: .leading, spacing: 7) {
        Text("TODAY’S BRIEF")
          .font(.system(size: 8, weight: .bold))
          .tracking(1)
          .foregroundStyle(NavigatorPalette.helmBlue)
        HStack(alignment: .firstTextBaseline) {
          Text("4").font(.system(size: 42, weight: .medium, design: .rounded))
          Text("updates are ready")
            .font(.system(size: 18, weight: .semibold, design: .rounded))
          Spacer()
        }
        Text("Everything else is on course. One authorization is expected.")
          .font(.system(size: 10))
          .foregroundStyle(palette.secondaryText)
        Text("Review Plan  →")
          .font(.system(size: 11, weight: .semibold))
          .foregroundStyle(.white)
          .padding(.horizontal, 14)
          .frame(height: 30)
          .background(NavigatorPalette.helmBlue, in: Capsule())
      }
      .padding(18)
      .background(
        LinearGradient(
          colors: [
            NavigatorPalette.helmBlue.opacity(0.13), NavigatorPalette.seaGlass.opacity(0.07),
          ],
          startPoint: .topLeading,
          endPoint: .bottomTrailing
        )
      )

      VStack(spacing: 0) {
        popoverBriefRow(
          "npm refresh needs review", symbol: "exclamationmark.triangle.fill",
          tone: NavigatorPalette.attention)
        Divider().overlay(palette.line).padding(.leading, 48)
        popoverBriefRow(
          "Application refresh · 67%", symbol: "waveform.path.ecg", tone: NavigatorPalette.seaGlass)
      }

      Divider().overlay(palette.line)
      HStack {
        Text("Open Dashboard")
        Spacer()
        Text("Find software…")
        Image(systemName: "gearshape")
      }
      .font(.system(size: 10, weight: .semibold))
      .foregroundStyle(palette.secondaryText)
      .padding(.horizontal, 18)
      .frame(height: 46)
      .background(palette.surface)
    }
    .frame(width: 400)
    .foregroundStyle(palette.primaryText)
    .background(palette.chrome)
    .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
    .overlay {
      RoundedRectangle(cornerRadius: 18, style: .continuous).stroke(palette.line, lineWidth: 1)
    }
    .shadow(color: .black.opacity(colorScheme == .dark ? 0.5 : 0.2), radius: 24, y: 11)
    .padding(28)
  }

  private func popoverBriefRow(_ title: String, symbol: String, tone: Color) -> some View {
    HStack(spacing: 11) {
      Image(systemName: symbol).font(.system(size: 11)).foregroundStyle(tone).frame(width: 18)
      Text(title).font(.system(size: 10, weight: .medium))
      Spacer()
      Image(systemName: "chevron.right")
        .font(.system(size: 8, weight: .bold))
        .foregroundStyle(palette.secondaryText)
    }
    .padding(.horizontal, 18)
    .frame(height: 44)
  }
}
