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

#[cfg(windows)]
pub use imp::ElevatedChild;
#[cfg(not(windows))]
pub use stub::ElevatedChild;

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
        imp::run_elevated(program, parameters)
    }
    #[cfg(not(windows))]
    {
        let _ = (program, parameters);
        Err(ExecError::Unsupported)
    }
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::os::windows::process::CommandExt as _;

    use windows::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, GetLastError, HANDLE};
    use windows::Win32::System::Threading::GetExitCodeProcess;
    use windows::Win32::UI::Shell::{
        SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW, ShellExecuteW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{SW_HIDE, SW_SHOWNORMAL};
    use windows::core::{HSTRING, PCWSTR, w};

    use super::{CaptureOutput, ExecError};

    /// Runs a child process without flashing a console window.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    /// Values at or below this from `ShellExecuteW` indicate an error.
    const SHELL_EXECUTE_MIN_SUCCESS: usize = 32;

    /// `GetExitCodeProcess` reports this value while the process is still
    /// running.
    const STILL_ACTIVE: u32 = 259;

    /// An owning handle to an elevated child process.
    pub struct ElevatedChild {
        /// The process `HANDLE`, stored as an integer so the type is `Send` and
        /// can be moved onto the worker thread. Reconstructed into a `HANDLE`
        /// for each API call.
        handle: isize,
    }

    // SAFETY: the stored value is an owned process handle. `ElevatedChild` has
    // exclusive ownership of it (closed once in `Drop`) and never aliases it, so
    // it is safe to move between threads.
    unsafe impl Send for ElevatedChild {}

    impl ElevatedChild {
        fn handle(&self) -> HANDLE {
            HANDLE(self.handle as *mut c_void)
        }

        /// Returns the process exit code if it has finished, or `None` while it
        /// is still running.
        ///
        /// # Errors
        ///
        /// Returns [`ExecError::Api`] if querying the process fails.
        pub fn try_exit_code(&self) -> Result<Option<i32>, ExecError> {
            let mut code: u32 = 0;
            // SAFETY: `self.handle()` is a valid process handle owned by this
            // struct, and `code` is a valid out-pointer.
            unsafe { GetExitCodeProcess(self.handle(), &raw mut code) }
                .map_err(|err| ExecError::Api(err.message()))?;
            if code == STILL_ACTIVE {
                return Ok(None);
            }
            #[expect(
                clippy::cast_possible_wrap,
                reason = "process exit codes round-trip through i32 as reported by the tool"
            )]
            Ok(Some(code as i32))
        }
    }

    impl Drop for ElevatedChild {
        fn drop(&mut self) {
            // SAFETY: the handle was returned by `ShellExecuteExW` with
            // `SEE_MASK_NOCLOSEPROCESS` and is closed exactly once here.
            let _ = unsafe { CloseHandle(self.handle()) };
        }
    }

    pub(super) fn run_elevated(
        program: &str,
        parameters: &str,
    ) -> Result<ElevatedChild, ExecError> {
        // The HSTRINGs must outlive the `ShellExecuteExW` call below.
        let file = HSTRING::from(program);
        let parameters = HSTRING::from(parameters);

        let mut info = SHELLEXECUTEINFOW {
            cbSize: u32::try_from(size_of::<SHELLEXECUTEINFOW>()).unwrap_or_default(),
            // Required so `hProcess` is populated and we can read the exit code.
            fMask: SEE_MASK_NOCLOSEPROCESS,
            lpVerb: w!("runas"),
            lpFile: PCWSTR(file.as_ptr()),
            lpParameters: PCWSTR(parameters.as_ptr()),
            nShow: SW_HIDE.0,
            ..Default::default()
        };

        // SAFETY: `info` is fully initialized and its string pointers reference
        // `file`/`parameters`, which outlive this call.
        let launched = unsafe { ShellExecuteExW(&raw mut info) };
        if let Err(err) = launched {
            // SAFETY: reads the calling thread's last-error code.
            let last = unsafe { GetLastError() };
            if last == ERROR_CANCELLED {
                return Err(ExecError::Declined);
            }
            return Err(ExecError::Api(err.message()));
        }

        if info.hProcess.is_invalid() {
            return Err(ExecError::Api(
                "ShellExecuteEx returned no process handle".to_owned(),
            ));
        }

        Ok(ElevatedChild {
            handle: info.hProcess.0 as isize,
        })
    }

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

#[cfg(not(windows))]
mod stub {
    use super::ExecError;

    /// Placeholder handle type on non-Windows platforms. Never constructed;
    /// [`super::run_elevated`] returns [`ExecError::Unsupported`] before any
    /// child could exist.
    pub struct ElevatedChild {
        _private: (),
    }

    impl ElevatedChild {
        /// Always fails: no elevated child can exist off Windows.
        ///
        /// # Errors
        ///
        /// Always returns [`ExecError::Unsupported`].
        pub fn try_exit_code(&self) -> Result<Option<i32>, ExecError> {
            Err(ExecError::Unsupported)
        }
    }
}
