# uv Read-Only Contract Fixtures

These are synthetic contract fixtures, not an inventory of the maintainer's Mac
or evidence of real tool installation/certification.

The header/child-row syntax and installed requirement annotation are checked
against the [uv 0.12.18 list implementation](https://github.com/astral-sh/uv/blob/0.12.18/crates/uv/src/commands/tool/list.rs)
and its [tool-list tests](https://github.com/astral-sh/uv/blob/0.12.18/crates/uv/tests/tool/tool_list.rs).
`latest.txt` deliberately combines an exact installed constraint with a newer
registry version. This is discovery data, not proof of an eligible upgrade.

Local uv 0.12.9 help confirmed `--show-version-specifiers`, `--outdated`,
`--color never`, `--no-progress`, and `--offline`. It did not establish an oldest
supported version or a complete adapter capability boundary.

No fixture implies support for `--show-paths`, other optional annotations, JSON,
or mutating commands. Error-path diagnostics are synthetic and are never copied
into parser error messages.
