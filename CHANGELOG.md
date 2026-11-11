# Changelog

Releases before 0.2.4 are listed on the [releases page](https://github.com/handsomefox/win-toolkit/releases).

## 0.4.7

- **Check system drive** checks the drive Windows is installed on. It always checked `C:`, so a
  Windows installed on another drive was never checked.
- **Create a System Restore point** reports success when Windows makes room for the new point by
  deleting the oldest one. Those runs were reported as failed.
- Open programs, files, and folders with COM initialized first, as Microsoft asks of callers of
  the Windows shell. Some file types open through shell extensions that need it.
- Opening a second copy of the app while the first has an update waiting no longer deletes the
  first copy's download.

## 0.4.6

- Report **Create a System Restore point** as failed when Windows creates no restore point, for
  example within 24 hours of the last one. 0.4.5 still reported those runs as successful.

## 0.4.5

- **Check for updates** in **About** checks GitHub right away as it opens the update window. It
  only opened the window.
- Cap each day's log file at 10 MiB, so a runaway log cannot fill the disk.
- Get the self-updater, the update window, logging, and the folder lookups from [handsomefox/app-internals](https://github.com/handsomefox/app-internals),
  which all five apps share, instead of a copy inside this app.

## 0.4.4

- Get the self-updater from its own repository, [handsomefox/app-update](https://github.com/handsomefox/app-update), instead of a copy
  inside this app. Checking for, downloading, and installing an update work as before.

## 0.4.3

- Keep a running operation in the output panel until it finishes. **Dismiss** and four other
  buttons could clear it mid-run, which unlocked **Restart to update** during `sfc` or `DISM`.
- Report **Create a System Restore point** as failed when Windows does not create one. It
  reported success.
- Fail the running operation when the background worker stops unexpectedly. The panel showed
  "Running…" forever.
- Label the **Open logs folder** run from **About** the same way as the one in **Diagnostics**.
- Refuse an app update whose checksum on GitHub cannot be read, instead of skipping that check.
- Retry putting the current version back when installing an app update fails.

## 0.4.2

- New installs check for a new version each time the app starts. Existing installs keep their
  setting. To change it, open **About** > **App updates**.
- Keep a found update visible after a restart. With a check period of a day or longer, a restart
  hid the new version until the next check was due.
- Remove the line in **App updates** that said no check had run, which appeared beside the time
  of the last check.

## 0.4.1

- Give Windows Toolkit its own icon, a wrench and a screwdriver, in place of the Windows Cleaner
  broom.
- Update eframe and egui_extras to 0.37. The first launch after this update opens at the default
  window size and position, and the output panel returns to its default height. Later launches
  remember both again.
- Update flate2 to 1.1.10, serde to 1.0.229, serde_json to 1.0.154, sha2 to 0.11.1,
  windows-registry to 0.100.0, and zip to 9.2.0.

## 0.4.0

- Windows Toolkit can now update itself from its GitHub releases. It checks once a day, and when
  a new version is out, a chip in the header names it. Click the chip to read what changed since
  your version, then download the update, skip that version, or leave it for later. The download
  must match its published SHA-256 checksums before it replaces the app, and a restart finishes
  the install. A restart waits for a running operation, so it never interrupts `sfc` or `DISM`.
  To check more or less often, or never, open **About** > **App updates**.
- Getting 0.4.0 itself is still a manual download. Later versions install from inside the app.

## 0.3.4

- Replace the yanked clipboard-win 5.4.1 with 5.4.3, and update tokio to 1.54.1.

## 0.3.3

- Update jiff to 0.2.38 and embed-resource to 3.0.12.

## 0.3.2

- Update thiserror to 2.0.21, and replace the yanked yoke-derive 0.8.3 with 0.8.4.

## 0.3.1

- Fix the repository link in the About section, which did not open.

## 0.3.0

- Update egui from 0.35 to 0.36, along with smaller dependencies.
- Build with Rust 1.98.1.

## 0.2.4

- Ship `win-toolkit-<version>-windows-x86_64.zip`, which holds a folder of the same name with
  `win-toolkit.exe`, `README.md`, and `LICENSE` in it, beside a `SHA256SUMS` file. The
  executable used to be `Windows Toolkit.exe`, and was also attached on its own. Windows still
  shows it as Windows Toolkit.
- Show the release version in the executable's properties. Releases 0.2.2 and 0.2.3 still
  said 0.2.1 there.
