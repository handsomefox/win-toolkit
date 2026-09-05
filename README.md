# Windows Toolkit

[![CI](https://github.com/handsomefox/win-toolkit/actions/workflows/ci.yml/badge.svg)](https://github.com/handsomefox/win-toolkit/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

A Windows desktop app that inspects your system and runs documented Windows maintenance and diagnostic commands. Every action says what it does before it runs, and none of them are placebo "optimizations".

## What it does

The app has seven sections.

**Overview.** A read-only summary of the system and hardware: OS build, uptime, CPU, memory, and free space per drive. You can export it as text for a support request.

**Health & Repair.** `sfc /scannow`, `DISM /RestoreHealth`, `chkdsk C: /scan`, analysis and cleanup of the WinSxS component store, Reset Windows Update, and Create a System Restore point. Each is a separate action with its own description and confirmation.

**Network.** Flush the DNS resolver cache, reset the Winsock catalog, reset the TCP/IP stack, release or renew the IP address, and view `ipconfig /all`.

**Startup.** List startup entries, and enable or disable them.

**Performance.** Switch power plans, add the Ultimate Performance plan, and turn Windows memory compression on or off. No invented speed-ups.

**Sandbox.** Build a Windows Sandbox configuration and launch it.

**Diagnostics.** Generate and open a battery report, and launch dxdiag, Event Viewer, Reliability Monitor, and the servicing logs.

## Safety model

The app itself runs unelevated, with an `asInvoker` manifest, so the read-only tools work without a UAC prompt. Each privileged operation launches its own elevated child process. You get a prompt when you run something that needs administrator rights, and not before.

Every operation wraps a documented, built-in Windows command or API. There are no undocumented registry tweaks, no arbitrary service disabling, and no performance claims the app cannot back up.

Before a privileged operation runs, the app shows what it does, roughly how long it takes, and what it costs you. A network reset needs a reboot afterward, for example, and Reset Windows Update renames the servicing folders rather than deleting them.

The long repair operations, `sfc` and `DISM`, are not cancelable, because interrupting them mid-run can leave the component store inconsistent. Their output streams to the log view and to the diagnostics log as it arrives.

## Diagnostics

The app writes logs to `%LOCALAPPDATA%\win-toolkit\logs\`. If something fails, attach the newest log file to your issue.

## Development

The portable logic builds and tests on any OS, including Linux. The GUI runs there too, but the maintenance and diagnostic operations call Windows commands and only work on Windows.

```sh
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

CI runs those three commands on Ubuntu and Windows, plus a native release build on Windows and `cargo audit`.

To cross-build the Windows 10/11 x86-64 app from Linux, use `cargo-xwin`:

```sh
cargo xwin build --workspace --release --target x86_64-pc-windows-msvc
```

To produce the portable executable, its SHA-256 checksum, and a ZIP under `dist/`:

```sh
bash scripts/package-windows.sh
```

The packaging script needs `cargo-xwin`, `zip`, and GNU `sha256sum`. It verifies the checksum file and the ZIP before it reports success.

## License

Licensed under the [MIT License](LICENSE). Bundled assets keep their own licenses: the [Inter](https://rsms.me/inter/) typeface under SIL OFL 1.1, and the [Phosphor](https://phosphoricons.com/) icons under MIT.
