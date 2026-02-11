# Helm macOS UI

SwiftUI menu bar utility for macOS 12+ (Monterey).

## Overview

Helm runs as a menu bar app (`LSUIElement`) with no Dock icon. The app uses an `NSApplicationDelegateAdaptor` to create an `NSStatusItem` in the system status bar.

## Building

```bash
cd apps/macos-ui
xcodebuild -project Helm.xcodeproj -scheme Helm -configuration Debug build
```

## Requirements

- Xcode 14+
- macOS 12.0 deployment target
- Swift 5.7
