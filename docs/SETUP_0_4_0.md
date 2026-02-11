# Setup Instructions for Helm 0.4.0 (SwiftUI Shell with XPC)

This milestone introduces the XPC Service architecture. The Rust core is now hosted in a separate XPC service process (`HelmService`) rather than embedded directly in the UI app.

## Prerequisites

1. Ensure you have Rust installed (`rustc 1.82+`).
2. Ensure you have Xcode 14+ installed.

## One-Time Xcode Setup

Open `apps/macos-ui/Helm.xcodeproj` in Xcode.

### 1. Add New Target: HelmService

1. File > New > Target.
2. Select **XPC Service** (under macOS > System Extension / Driver / XPC Service).
3. Product Name: `HelmService`.
4. Bundle Identifier: `com.jasoncavinder.Helm.HelmService` (Must match `Info.plist` and `HelmCore.swift`).
5. Finish.

### 2. Add Source Files

1. Right-click on the **Helm** (blue icon) project root.
2. **Add Files to "Helm"...**
3. Select `apps/macos-ui/Helm/Shared` folder.
   * Add to targets: **Helm** AND **HelmService**.
4. Select `service/macos-service/Sources` folder (you may need to create a group "Service" first).
   * Add to targets: **HelmService** ONLY.
   * Note: Replace the default `main.swift` created by the template with the one from `Sources`.
5. Select `service/macos-service/HelmService-Bridging-Header.h`.
   * Add to targets: **HelmService** ONLY.

### 3. Configure HelmService Build Settings

Select the **HelmService** target:

1. **Bridging Header**:
   * Set `Objective-C Bridging Header` to `$(PROJECT_DIR)/../../service/macos-service/HelmService-Bridging-Header.h`.
2. **Search Paths**:
   * Set `Header Search Paths` to `$(SRCROOT)/Generated` (Recursive: No).
   * Set `Library Search Paths` to `$(SRCROOT)/Generated` (Recursive: No).
3. **Rust Build Script**:
   * Add a Run Script Phase (before Compile Sources):
     ```bash
     "$SRCROOT/scripts/build_rust.sh"
     ```
4. **Link Binary With Libraries**:
   * Add `apps/macos-ui/Generated/libhelm_ffi.a`.

### 4. Configure Helm (UI) Target

Select the **Helm** target:

1. **Remove Rust Linking**:
   * Remove `libhelm_ffi.a` from Link Binary With Libraries.
   * Remove the Rust Build Script (it's now in the Service).
   * Clear the Bridging Header setting (UI doesn't speak C anymore).
2. **Embed XPC Service**:
   * Build Phases > **Embed XPC Services**.
   * Ensure `HelmService.xpc` is listed here.

## Verification

1. Run the **Helm** scheme.
2. The app should launch.
3. It will attempt to connect to XPC.
4. Verify functionality (Refresh, Lists) works as before but now via XPC.

