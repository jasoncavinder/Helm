# Setup Instructions for Helm 0.4.0 (SwiftUI Shell)

This milestone introduces the Rust FFI layer and SwiftUI shell. Since Xcode project files (`.xcodeproj`) cannot be easily modified by automation, you must perform a one-time manual setup to link the Rust core.

## Prerequisites

1. Ensure you have Rust installed (`rustc 1.82+`).
2. Ensure you have Xcode 14+ installed.

## One-Time Xcode Setup

Open `apps/macos-ui/Helm.xcodeproj` in Xcode.

### 1. Add Build Script Phase

1. Select the **Helm** target.
2. Go to **Build Phases**.
3. Click `+` > **New Run Script Phase**.
4. Name it "Build Rust Core".
5. Drag it to be the **first** phase (before "Compile Sources").
6. Set the script content to:
   ```bash
   "$SRCROOT/scripts/build_rust.sh"
   ```
7. Uncheck "Based on dependency analysis" (or ensure it runs when needed).

### 2. Configure Bridging Header

1. Go to **Build Settings**.
2. Search for "Objective-C Bridging Header".
3. Set it to: `Helm/Helm-Bridging-Header.h` (relative to project root, might need adjustment depending on where Xcode thinks root is. Try `$(PROJECT_DIR)/Helm/Helm-Bridging-Header.h`).
4. Search for "Header Search Paths".
5. Add `$(SRCROOT)/Generated` with "Recursive" set to **No**.

### 3. Link Rust Library

1. Go to **Build Phases** > **Link Binary With Libraries**.
2. Click `+` > **Add Other...** > **Add Files...**.
3. Navigate to `apps/macos-ui/Generated`.
   > **Note:** If this directory doesn't exist yet, run `apps/macos-ui/scripts/build_rust.sh` in your terminal first.
4. Select `libhelm_ffi.a`.

### 4. Library Search Paths

1. Go to **Build Settings**.
2. Search for "Library Search Paths".
3. Add `$(SRCROOT)/Generated`.

## Verification

1. Select "My Mac" as the destination.
2. Hit **Run** (Cmd+R).
3. The app should launch in the menu bar.
4. Click the Helm icon -> You should see "Initializing Core..." then the Task/Package lists.
