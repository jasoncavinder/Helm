# Helm Design Lab

The Design Lab is a non-shipping SwiftUI executable for rendering reproducible Helm visual proposals. It is deliberately separate from the Xcode application target: proposal code and assets cannot affect production behavior, bundle size, localization, signing, or release artifacts.

From the repository root, render every proposal in light and dark appearance:

```sh
swift run --package-path tools/design-lab HelmDesignLab \
  --output docs/app-design/proposals/v019-visual-direction/renders
```

The renderer uses native SwiftUI and SF Symbols on the repository's macOS 13 minimum. Generated PNGs are committed with the proposal so GitHub review does not require a local build.

## Review lifecycle

1. A proposal PR defines the comparison question and holds production behavior constant.
2. Each direction is rendered from the same fixture in light and dark appearance.
3. Reviewers comment on the PR, the proposal document, or a specific rendered asset.
4. The decision log records `Approved`, `Needs revision`, or `Rejected`, plus the reasoning.
5. Only an approved direction may be translated into a production implementation slice.

The Design Lab is not a second app, a component framework, or a source of business logic. It may approximate data for visual comparison only.
