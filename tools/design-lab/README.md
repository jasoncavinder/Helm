# Helm Design Lab

The Design Lab is a non-shipping SwiftUI executable for rendering reproducible Helm experience proposals. It is deliberately separate from the Xcode application target: proposal code and assets cannot affect production behavior, bundle size, localization, signing, or release artifacts.

From the repository root, render the Dashboard and popover workflow set:

```sh
swift run --package-path tools/design-lab HelmDesignLab \
  --output docs/app-design/proposals/v019-visual-direction/renders
```

The renderer uses native SwiftUI and SF Symbols on the repository's macOS 13 minimum. Generated PNGs are committed with the proposal so GitHub review does not require a local build.

## Review lifecycle

1. A proposal PR defines the experience question and the common jobs it must support.
2. Renders demonstrate distinct workflows and states, not merely palette variations.
3. Reviewers comment on the PR, proposal document, or a specific rendered asset.
4. The decision log records `Approved`, `Needs revision`, or `Rejected`, plus reasoning.
5. Only an approved experience direction may change canonical IA or enter production implementation.

The Design Lab is not a second app, a component framework, or a source of business logic. It uses synthetic fixtures for visual and workflow comparison only.
