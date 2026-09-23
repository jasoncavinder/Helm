# uv Read-Only Contract Fixtures

The text files are synthetic contract fixtures, not an inventory of the
maintainer's Mac or evidence of real tool installation/certification.

The header/child-row syntax and installed requirement annotation are checked
against the [uv 0.12.18 list implementation](https://github.com/astral-sh/uv/blob/0.12.18/crates/uv/src/commands/tool/list.rs)
and its [tool-list tests](https://github.com/astral-sh/uv/blob/0.12.18/crates/uv/tests/tool/tool_list.rs).
`latest.txt` deliberately combines an exact installed constraint with a newer
registry version. This is discovery data, not proof of an eligible upgrade.

Local uv 0.12.9 help confirmed `--show-version-specifiers`, `--outdated`,
`--color never`, `--no-progress`, and `--offline`. It did not establish an oldest
supported version or a complete adapter capability boundary.

No text fixture implies support for `--show-paths`, other optional annotations,
JSON, or mutating commands. Error-path diagnostics are synthetic and are never
copied into parser error messages.

`isolated_lifecycle.py` is the fixture driver for the opt-in
`uv_tool_real_contract` Rust test. It generates local, dependency-free wheels
with standard-library ZIP tooling, installs them only into a fresh isolated
store, and captures real uv output for the production parser. It exercises
constraint-preserving upgrades, exact pins, multiple entrypoints, failed
installation, a broken synthetic receipt, restoration, and scoped uninstall.
The Rust command builder supplies the list arguments. All commands use a clean
environment, no inherited configuration, disabled networking/Python downloads,
and a 30-second subprocess deadline. Home, tool/bin/cache/Python/config/temp
directories belong to the fresh run; no installed host tools are queried or
modified. Artifacts are retained rather than automatically cleaned up.

The runner requires explicit absolute uv and Python executable paths and is
ignored in ordinary Cargo test runs. Invocation and bounded evidence are in
`docs/validation/uv-global-tools-foundation.md`. This is upstream-command/parser
coverage, not certification of a registered Helm adapter or its orchestration.
