# Windows Toolkit

[![CI](https://github.com/handsomefox/win-toolkit/actions/workflows/ci.yml/badge.svg)](https://github.com/handsomefox/win-toolkit/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

A Windows desktop app that inspects your system and runs documented Windows maintenance and diagnostic tasks — every action explained, reversible where possible, and never a placebo "optimizer".

## Features

- **Overview** — read-only system and hardware summary (OS build, uptime, CPU, memory, per-drive space) you can export as text for support requests.
- **Health & Repair** — `sfc /scannow`, `DISM /RestoreHealth`, WinSxS component-store cleanup, CHKDSK and disk/SMART health, Reset Windows Update, and Create Restore Point — each an individual, described, confirmed action.
- **Network** — flush DNS, reset Winsock, reset TCP/IP, release/renew IP, and view `ipconfig /all`.
- **Startup** — list startup entries and enable or disable them.
- **Performance** — switch power plans and toggle documented Windows settings (e.g. memory compression). No fake speed-ups.
- **Sandbox** — build and launch a Windows Sandbox configuration.
- **Diagnostics** — battery report and quick launchers for dxdiag, Event Viewer, Reliability Monitor, and the servicing logs.

## Safety model

- The app runs **unelevated** (`asInvoker`); read-only tools work without a prompt. Each privileged operation launches an **elevated child process** (a UAC prompt), so administrative rights are requested only when you run something that needs them.
- Every operation is wrapped around a **documented, built-in Windows command or API** — no undocumented registry tweaks, no arbitrary service disabling, no misleading performance claims.
- Before a privileged operation runs, the app shows **what it does, how long it may take, and its consequences** (for example, that a network reset requires a reboot, or that Reset Windows Update renames — not deletes — servicing folders).
- Long repair operations (SFC/DISM) are not interrupted mid-run, and their full output is streamed to the log view and written to the diagnostics log.

## Diagnostics

The app writes logs to `%LOCALAPPDATA%\win-toolkit\logs\`. If something fails, attach the latest log file to your issue.

## Development

Portable logic builds and tests on Linux; the GUI also runs there for development, but the maintenance and diagnostic operations are Windows-only:

```sh
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Build the Windows 10/11 x86-64 application from Linux with `cargo-xwin`:

```sh
cargo xwin build --workspace --release --target x86_64-pc-windows-msvc
```

Create the portable executable, checksum, and ZIP under `dist/`:

```sh
bash scripts/package-windows.sh
```

The packaging script requires `cargo-xwin`, `zip`, and GNU `sha256sum`; it
verifies both the generated checksum and ZIP before returning success.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Please report security-sensitive issues according to [SECURITY.md](SECURITY.md).

## License

Licensed under the [MIT License](LICENSE). Bundled assets keep their own licenses: [Inter](https://rsms.me/inter/) (SIL OFL 1.1) and [Phosphor](https://phosphoricons.com/) icons (MIT).
