---
title: Installation
description: Build and run Helm from source.
---

Helm is pre-1.0 software and is currently built from source. Binary distribution is planned for a future release.

## Prerequisites

- macOS 12 (Monterey) or later
- Xcode 14+
- Rust stable toolchain (2024 edition)

## Build from source

### 1. Clone the repository

```bash
git clone https://github.com/jasoncavinder/Helm.git
cd Helm
```

### 2. Build and test the Rust core

```bash
cd core/rust
cargo test --lib       # unit tests
cargo test --test '*'  # integration tests
cargo build            # build the library
```

### 3. Build the macOS app

```bash
cd apps/macos-ui
xcodebuild -project Helm.xcodeproj -scheme Helm -configuration Debug build
```

Or open `apps/macos-ui/Helm.xcodeproj` in Xcode and run the **Helm** scheme. The build script automatically compiles the Rust FFI library and generates version headers.

### 4. Run

Launch the built app. Helm appears as a menu bar icon — click it to open the floating panel. There is no Dock icon by design.

## Verify

After launching, click **Refresh** to populate the package list. You should see packages from any installed and supported manager (Homebrew, mise, rustup).
