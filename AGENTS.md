# Repository guidelines

The [README](README.md) covers what the app does, its safety model, and the build and check commands. This page covers the rules for changing it.

## Where code goes

The workspace is layered so that everything portable stays testable on Linux:

- `toolkit-core` holds the operation model and the built-in catalogs, command-line construction, the sandbox configuration, startup entries, and system information types. Operations are plain data.
- `toolkit-platform` wraps the Windows APIs: process execution and elevation, known folders, startup entries, and system information.
- `toolkit-app` is the egui desktop binary, and the only crate that may depend on egui.

An operation is data in `toolkit-core`, not a function that does the work. Adding one means adding a catalog entry, not a new execution path.

## Every operation wraps a documented Windows command

No undocumented registry tweaks, no arbitrary service disabling, no performance claims the app cannot back up. If an operation cannot point at Microsoft documentation for what it runs, it does not belong here. That rule is the whole reason to use this over the tools it replaces.

## Do not weaken the elevation model

The app runs unelevated with an `asInvoker` manifest, so the read-only tools work without a UAC prompt. Each privileged operation launches its own elevated child process. Do not elevate the whole app to make something simpler.

Before a privileged operation runs, the confirmation dialog states what it does, roughly how long it takes, and what it costs the user. When you add an operation, write that description and its consequences as carefully as its command line: that text is the only thing standing between a user and a change they did not expect.

`sfc` and `DISM` are deliberately not cancelable, because interrupting them mid-run can leave the component store inconsistent. Do not add cancellation to them.

## Tests

Unit tests live beside their implementations in `#[cfg(test)]` modules. Because operations are plain data, the command line an operation builds is testable off Windows. Cover that, and cover the rejection path for anything that parses output or resolves a path.

CI cannot reach the elevation flow. Exercise it by hand on Windows, along with output capture, cancellation, and any operation that changes system state.
