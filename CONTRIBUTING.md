# Contributing

Thanks for helping improve win-toolkit.

## Before opening a change

- Use an issue for substantial behavior or architecture changes.
- Keep platform-neutral logic in `toolkit-core` and Windows APIs in `toolkit-platform`; `toolkit-app` is the only crate that may depend on egui.
- Do not weaken the safety model for convenience: unelevated-by-default operation, per-action elevation, and the confirmation dialog that states each privileged operation's effect and consequences before it runs.
- Every operation must wrap a documented, built-in Windows command or API. No undocumented tweaks, arbitrary service disabling, or performance claims. New operations should describe exactly what they run and why it is safe.

## Verification

Run the portable checks before submitting a pull request:

```sh
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Changes to Windows-only behavior should also pass:

```sh
cargo xwin build --workspace --release --target x86_64-pc-windows-msvc
```

Describe any manual Windows testing in the pull request, especially for elevation flows, command output capture and cancellation, and any operation that changes system state (services, network configuration, restore points, or startup entries).
