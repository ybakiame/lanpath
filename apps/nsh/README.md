# Lanpath NSH

An app written in Rust: its UI is `src/lib.rs`, and `nsh.json` names the root component (`"root": "App"`).

```sh
nsh run                      # this machine's backend; --target ios, android, web, macos, linux or windows
nsh check                    # cargo check of the crate
nsh test                     # the crate's tests (cargo test): a headless test of the root
nsh test nsh.json --trace    # headless: what the root mounts, and the time of each commit
nsh package --target web     # or ios, android, macos, linux, windows
```

`nsh run` and `nsh package` mount the root on every platform, the browser included, without QuickJS: the app is Rust
and native code from start to end (in the browser, with NSH's page script). Rust edits rebuild and restart the app.

```sh
cargo run
```

This starts the root directly on the machine's native desktop backend: GTK 4 on Linux, AppKit on macOS, Win32 on
Windows. Install the backend's development libraries before building.

The test in `tests/` runs the root with `nsh_testing::Headless` (NSH 0.5.0): it presses “解析共享订阅” and checks
the shared parse result on screen. It needs no QuickJS and no Node — `nsh test` runs it with cargo test, and so does
a plain

```sh
cargo test
```

The `nsh` commands build the app's runners from the NSH crates `Cargo.toml` depends on: crates.io's at the version of
the `nsh` you run (keep the `nsh-*` dependencies at that version when you upgrade `nsh`), or an NSH checkout's by path.

NSH-generated files stay in `.nsh/`: runner sources in `.nsh/rust/` and default packages in `.nsh/out/`.
Cargo keeps its normal `target/` directory and respects your existing `CARGO_TARGET_DIR`.

We recommend building with [huacnlee/cargo](https://github.com/huacnlee/cargo), the Cargo NSH is developed with: a
drop-in fork that builds each dependency once for all your projects and Git worktrees and keeps its build directory
under a size limit on its own, with nothing to configure. It builds under `~/.cargo/targets/` and still puts the
binaries in `target/`; it replaces `~/.cargo/bin/cargo`, and its README says how to put rustup's back.

```sh
curl -fsSL https://huacnlee.github.io/cargo/install.sh | sh     # Windows: irm https://huacnlee.github.io/cargo/install.ps1 | iex
```
