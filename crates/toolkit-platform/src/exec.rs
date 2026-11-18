//! Launching elevated child processes.
//!
//! The application runs unelevated (`asInvoker`). To perform a privileged
//! operation it launches a child process elevated via `ShellExecuteEx` with the
//! `runas` verb, which triggers a UAC prompt. The child's console output is
//! redirected to a file (see [`toolkit_core::elevated_cmd_parameters`]) that the
//! GUI tails; this module owns the process handle and reports the exit code.
//!
//! A medium-integrity process cannot signal or terminate a high-integrity child,
//! so there is deliberately no "kill" here: elevated operations run to
//! completion.

use thiserror::Error;

/// A failure while launching or polling an elevated child process.
#[derive(Debug, Error)]
pub enum ExecError {
    /// The user dismissed the UAC prompt (declined elevation). This is a normal
    /// outcome, distinct from an operation failure.
    #[error("elevation was declined")]
    Declined,
    /// Elevated execution was attempted on a non-Windows platform.
    #[error("elevated execution is only available on Windows")]
    Unsupported,
    /// A Windows API call failed.
    #[error("Windows API error: {0}")]
    Api(String),
}

/// An owning handle to an elevated child process.
pub struct ElevatedChild {
    child: app_elevation::Child,
}

impl ElevatedChild {
    /// Returns the process exit code if it has finished, or `None` while it
    /// is still running.
    ///
    /// # Errors
    ///
    /// Returns [`ExecError::Api`] if querying the process fails.
    pub fn try_exit_code(&self) -> Result<Option<i32>, ExecError> {
        let code = self.child.try_wait().map_err(elevation_error)?;
        #[expect(
            clippy::cast_possible_wrap,
            reason = "process exit codes round-trip through i32 as reported by the tool"
        )]
        Ok(code.map(|code| code as i32))
    }
}

/// Maps a launch or wait failure onto this module's errors, keeping a
/// declined UAC prompt distinct.
fn elevation_error(error: app_elevation::Error) -> ExecError {
    match error {
        app_elevation::Error::Cancelled => ExecError::Declined,
        app_elevation::Error::Unsupported => ExecError::Unsupported,
        other => ExecError::Api(other.to_string()),
    }
}

/// The captured result of an unelevated command.
pub struct CaptureOutput {
    /// The process exit code (`-1` if it could not be determined).
    pub code: i32,
    /// Combined stdout and stderr bytes, still to be decoded.
    pub output: Vec<u8>,
}

/// Runs `program` with `args` unelevated, capturing its combined output and exit
/// code once it finishes. Intended for quick, read-only or user-scope commands.
///
/// # Errors
///
/// Returns [`ExecError::Unsupported`] on non-Windows platforms, or
/// [`ExecError::Api`] if the process could not be launched.
pub fn run_capture(program: &str, args: &[String]) -> Result<CaptureOutput, ExecError> {
    #[cfg(windows)]
    {
        imp::run_capture(program, args)
    }
    #[cfg(not(windows))]
    {
        let _ = (program, args);
        Err(ExecError::Unsupported)
    }
}

/// Opens `target` (a program, file, or folder) with the shell `open` verb,
/// unelevated, without capturing output.
///
/// # Errors
///
/// Returns [`ExecError::Unsupported`] on non-Windows platforms, or
/// [`ExecError::Api`] if the shell reports a launch failure.
pub fn launch(target: &str, args: &[String]) -> Result<(), ExecError> {
    #[cfg(windows)]
    {
        imp::launch(target, args)
    }
    #[cfg(not(windows))]
    {
        let _ = (target, args);
        Err(ExecError::Unsupported)
    }
}

/// Launches `program` with `parameters` elevated (via the `runas` verb),
/// returning a handle to the running child.
///
/// # Errors
///
/// Returns [`ExecError::Declined`] if the user dismisses the UAC prompt,
/// [`ExecError::Unsupported`] on non-Windows platforms, or [`ExecError::Api`]
/// for any other Windows API failure.
pub fn run_elevated(program: &str, parameters: &str) -> Result<ElevatedChild, ExecError> {
    #[cfg(windows)]
    {
        // The elevated command line names tools such as `sfc` without a
        // path, and the shell looks in the working directory first.
        let child = app_elevation::Command::new(program)
            .raw_arg(parameters)
            .current_dir(crate::paths::system_directory())
            .show(app_elevation::Show::Hidden)
            .spawn()
            .map_err(elevation_error)?;
        Ok(ElevatedChild { child })
    }
    #[cfg(not(windows))]
    {
        let _ = (program, parameters);
        Err(ExecError::Unsupported)
    }
}

#[cfg(windows)]
mod imp {
    use std::os::windows::process::CommandExt as _;

    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::core::{HSTRING, PCWSTR, w};

    use super::{CaptureOutput, ExecError};

    /// Runs a child process without flashing a console window.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    /// Values at or below this from `ShellExecuteW` indicate an error.
    const SHELL_EXECUTE_MIN_SUCCESS: usize = 32;

    pub(super) fn run_capture(program: &str, args: &[String]) -> Result<CaptureOutput, ExecError> {
        let output = std::process::Command::new(program)
            .args(args)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|err| ExecError::Api(err.to_string()))?;
        let mut bytes = output.stdout;
        bytes.extend_from_slice(&output.stderr);
        Ok(CaptureOutput {
            code: output.status.code().unwrap_or(-1),
            output: bytes,
        })
    }

    pub(super) fn launch(target: &str, args: &[String]) -> Result<(), ExecError> {
        let file = HSTRING::from(target);
        // Kept alive for the duration of the call when non-empty.
        let parameters = HSTRING::from(args.join(" "));
        let parameters = if args.is_empty() {
            PCWSTR::null()
        } else {
            PCWSTR(parameters.as_ptr())
        };

        let _com = app_elevation::ComApartment::enter();
        // SAFETY: `file`/`parameters` outlive the call and the pointers are
        // valid; a null directory and default show command are accepted.
        let instance = unsafe {
            ShellExecuteW(
                None,
                w!("open"),
                PCWSTR(file.as_ptr()),
                parameters,
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        if instance.0.addr() <= SHELL_EXECUTE_MIN_SUCCESS {
            return Err(ExecError::Api(format!(
                "ShellExecute failed with code {}",
                instance.0.addr()
            )));
        }
        Ok(())
    }
}
