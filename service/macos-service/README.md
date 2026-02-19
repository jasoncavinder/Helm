# macOS Service (Legacy Scaffold)

This directory contains legacy scaffold artifacts from early project setup. The actual XPC service implementation lives at:

```
apps/macos-ui/HelmService/
```

The XPC service hosts the Rust FFI layer (`helm-ffi`) in a separate unsandboxed process, providing:

- 25 XPC protocol methods (package queries, task management, manager operations, settings, pin management)
- Code-signing validation on all connections (team ID verification via SecCode)
- Graceful reconnection with exponential backoff (2s base, doubling to 60s cap)
- Timeout enforcement on all calls (30s data fetches, 300s mutations)
- JSON interchange over the XPC boundary

See `apps/macos-ui/Helm/Shared/HelmServiceProtocol.swift` for the protocol definition.
