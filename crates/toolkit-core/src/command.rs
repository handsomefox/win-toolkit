//! Portable helpers for building the elevated command line and decoding the
//! console output captured from a redirected child process.
//!
//! These are the fiddly, easy-to-get-wrong parts of the elevated-child
//! mechanism, so they live here (platform-neutral) and are unit-tested on any
//! host. Only the `ShellExecuteEx` syscall itself is Windows-specific and lives
//! in `toolkit-platform`.

use std::borrow::Cow;

/// A program and its arguments, used to describe what an [`crate::Operation`]
/// runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    /// The executable to run (e.g. `sfc`, `dism`). Resolved via the child
    /// `cmd.exe`'s `PATH`.
    pub program: String,
    /// The arguments passed to `program`, one token per element.
    pub args: Vec<String>,
}

impl CommandSpec {
    /// Builds a spec from a program name and its arguments.
    #[must_use]
    pub fn new(program: &str, args: &[&str]) -> Self {
        Self {
            program: program.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        }
    }
}

/// The program name operations give Windows PowerShell. The app swaps in its
/// full path with [`CommandLine::with_program_path`]: it lives outside the
/// system directory, so `cmd.exe` would look a bare name up on `PATH`, where
/// a folder the user can write to may come first.
pub const POWERSHELL: &str = "powershell.exe";

/// A command line to run through `cmd.exe`, either as a structured program plus
/// arguments or as a raw multi-step script (needed for sequences such as
/// resetting Windows Update).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandLine {
    /// A single program invoked with discrete arguments.
    Program(CommandSpec),
    /// A raw `cmd.exe` command line, used for multi-step sequences. The caller
    /// is responsible for its correctness; the captured exit code is `cmd.exe`'s.
    Script(String),
}

impl CommandLine {
    /// Convenience constructor for a single-program command line.
    #[must_use]
    pub fn program(program: &str, args: &[&str]) -> Self {
        CommandLine::Program(CommandSpec::new(program, args))
    }

    /// This command line, running `path` where it would run `program`.
    #[must_use]
    pub fn with_program_path(&self, program: &str, path: &str) -> Self {
        match self {
            CommandLine::Program(spec) if spec.program == program => {
                CommandLine::Program(CommandSpec {
                    program: path.to_owned(),
                    args: spec.args.clone(),
                })
            }
            other => other.clone(),
        }
    }

    /// Renders the command portion (before any redirection): a quoted program
    /// and arguments, or the raw script text.
    #[must_use]
    fn render(&self) -> String {
        match self {
            CommandLine::Program(spec) => {
                let mut rendered = quote_if_needed(&spec.program).into_owned();
                for arg in &spec.args {
                    rendered.push(' ');
                    rendered.push_str(&quote_if_needed(arg));
                }
                rendered
            }
            CommandLine::Script(raw) => raw.clone(),
        }
    }
}

/// Quotes a token only when it needs it: when it is empty, contains
/// whitespace, or contains a character `cmd.exe` would act on, such as the `)`
/// that ends the group [`elevated_cmd_parameters`] wraps the command in.
/// Quoting tokens that do not need it can change how some programs parse their
/// own command line, so it is applied sparingly.
fn quote_if_needed(token: &str) -> Cow<'_, str> {
    let special = |c: char| c.is_whitespace() || "()&|<>^".contains(c);
    if token.is_empty() || token.chars().any(special) {
        Cow::Owned(format!("\"{token}\""))
    } else {
        Cow::Borrowed(token)
    }
}

/// Builds the `lpParameters` string for launching `command` through `cmd.exe`,
/// redirecting both stdout and stderr to `output_path`.
///
/// The result is `/d /c "(<command>) > "<output_path>" 2>&1"`. `/d` disables
/// Command Processor `AutoRun` registry commands in the elevated process.
/// `cmd.exe`'s `/c` quote
/// handling strips the outer quote pair (there are more than two quotes and a
/// redirection operator between them), leaving the inner command line intact —
/// including the quoted output path, which is essential because it lives under a
/// user profile that may contain spaces (`C:\Users\John Doe\...`).
#[must_use]
pub fn elevated_cmd_parameters(command: &CommandLine, output_path: &str) -> String {
    // The output path is always quoted: it is a filesystem path that routinely
    // contains spaces.
    // Group scripts so redirection applies to every command in the sequence.
    format!(r#"/d /c "({}) > "{output_path}" 2>&1""#, command.render())
}

/// Decodes bytes captured from a redirected console into a `String`.
///
/// Windows tools disagree on encoding when their output is redirected: `sfc`
/// writes UTF-16LE (with a byte-order mark), while others write UTF-8 or ANSI.
/// This detects a UTF-16LE/UTF-8 BOM, falls back to a NUL-byte heuristic for
/// BOM-less UTF-16LE, and otherwise decodes as UTF-8 (lossily).
#[must_use]
pub fn decode_console_output(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return decode_utf16le(&bytes[2..]);
    }
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(&bytes[3..]).into_owned();
    }
    if looks_like_utf16le(bytes) {
        return decode_utf16le(bytes);
    }
    String::from_utf8_lossy(bytes).into_owned()
}

fn decode_utf16le(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&pair| u16::from_le_bytes(pair))
        .collect();
    String::from_utf16_lossy(&units)
}

/// Heuristic for BOM-less UTF-16LE: ASCII text encoded as UTF-16LE has a NUL as
/// every second byte. We sample the leading bytes and look for that pattern.
#[must_use]
pub fn looks_like_utf16le(bytes: &[u8]) -> bool {
    let sample = &bytes[..bytes.len().min(64)];
    if sample.len() < 4 {
        return false;
    }
    let odd_nuls = sample
        .iter()
        .skip(1)
        .step_by(2)
        .filter(|byte| **byte == 0)
        .count();
    let odd_total = sample.len() / 2;
    odd_total > 0 && odd_nuls * 2 >= odd_total
}

/// Decodes captured console bytes into display lines, collapsing carriage-return
/// progress overwrites.
///
/// Progress-reporting tools (SFC, DISM) redraw a percentage on one line using
/// `\r`; when captured to a file this yields lines like
/// `10%\r20%\r30% complete`. We keep only the text after the final `\r` in each
/// line so the display shows the latest value rather than the overwrite history.
#[must_use]
pub fn console_lines(bytes: &[u8]) -> Vec<String> {
    decode_console_output(bytes)
        .split('\n')
        .map(|line| {
            // Drop the CR from a CRLF line ending first; otherwise the split
            // below would keep the empty segment after that trailing `\r` and
            // wipe the line. Any remaining internal `\r` are progress
            // overwrites, so keep the final segment.
            let line = line.strip_suffix('\r').unwrap_or(line);
            let latest = line.rsplit('\r').next().unwrap_or(line);
            latest.trim_end().to_owned()
        })
        .collect()
}

/// Tidies decoded output lines for display: drops leading blank lines, collapses
/// runs of blank lines to a single blank, and trims trailing blanks.
///
/// Redirected tools such as `sfc` emit many blank and padding lines around their
/// real output; without this the meaningful text is pushed far down the view.
#[must_use]
pub fn compact_lines(lines: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    for line in lines {
        if line.trim().is_empty() {
            // Skip leading blanks and collapse consecutive blanks into one.
            if out.last().is_none_or(String::is_empty) {
                continue;
            }
            out.push(String::new());
        } else {
            out.push(line.clone());
        }
    }
    while out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameters_quote_only_spaced_tokens_and_always_the_path() {
        let command = CommandLine::program("sfc", &["/scannow"]);
        let params = elevated_cmd_parameters(
            &command,
            r"C:\Users\John Doe\AppData\Local\win-toolkit\runs\sfc.log",
        );
        assert_eq!(
            params,
            r#"/d /c "(sfc /scannow) > "C:\Users\John Doe\AppData\Local\win-toolkit\runs\sfc.log" 2>&1""#
        );
    }

    #[test]
    fn parameters_quote_arguments_that_contain_spaces() {
        let command = CommandLine::program(
            "schtasks.exe",
            &["/Run", "/TN", r"\Microsoft\Windows\Some Task", ""],
        );
        let params = elevated_cmd_parameters(&command, r"C:\out.log");
        assert_eq!(
            params,
            r#"/d /c "(schtasks.exe /Run /TN "\Microsoft\Windows\Some Task" "") > "C:\out.log" 2>&1""#
        );
    }

    #[test]
    fn parameters_quote_arguments_cmd_would_parse() {
        let command = CommandLine::program(
            "powershell.exe",
            &["-NoProfile", "-Command", "(Get-MMAgent).MemoryCompression"],
        );
        assert_eq!(
            elevated_cmd_parameters(&command, r"C:\out.txt"),
            r#"/d /c "(powershell.exe -NoProfile -Command "(Get-MMAgent).MemoryCompression") > "C:\out.txt" 2>&1""#
        );
    }

    #[test]
    fn program_path_replaces_only_the_named_program() {
        let path = r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe";
        let command = CommandLine::program(POWERSHELL, &["-Command", "Get-MMAgent"])
            .with_program_path(POWERSHELL, path);
        assert_eq!(
            elevated_cmd_parameters(&command, r"C:\out.txt"),
            format!(r#"/d /c "({path} -Command Get-MMAgent) > "C:\out.txt" 2>&1""#)
        );
        let other = CommandLine::program("sfc", &["/scannow"]);
        assert_eq!(other.with_program_path(POWERSHELL, path), other);
    }

    #[test]
    fn parameters_redirect_the_complete_script() {
        let command = CommandLine::Script("net stop wuauserv & net start wuauserv".to_owned());
        let params = elevated_cmd_parameters(&command, r"C:\out.log");
        assert_eq!(
            params,
            r#"/d /c "(net stop wuauserv & net start wuauserv) > "C:\out.log" 2>&1""#
        );
    }

    #[test]
    fn decodes_utf16le_with_bom() {
        // "Hi" as UTF-16LE with BOM.
        let bytes = [0xFF, 0xFE, b'H', 0x00, b'i', 0x00];
        assert_eq!(decode_console_output(&bytes), "Hi");
    }

    #[test]
    fn decodes_bomless_utf16le_via_heuristic() {
        let mut bytes = Vec::new();
        for ch in "There is progress".chars() {
            bytes.push(ch as u8);
            bytes.push(0x00);
        }
        assert_eq!(decode_console_output(&bytes), "There is progress");
    }

    #[test]
    fn decodes_utf8_by_default() {
        assert_eq!(decode_console_output(b"plain ascii"), "plain ascii");
    }

    #[test]
    fn compact_lines_trims_and_collapses_blanks() {
        let input = [
            String::new(),
            String::new(),
            "a".to_owned(),
            String::new(),
            String::new(),
            "b".to_owned(),
            String::new(),
            String::new(),
        ];
        assert_eq!(
            compact_lines(&input),
            vec!["a".to_owned(), String::new(), "b".to_owned()]
        );
    }

    #[test]
    fn console_lines_collapse_carriage_return_progress() {
        let raw = b"Beginning scan.\n10%\r20%\r30% complete\nDone.\n";
        let lines = console_lines(raw);
        assert_eq!(
            lines,
            vec![
                "Beginning scan.".to_owned(),
                "30% complete".to_owned(),
                "Done.".to_owned(),
                String::new(),
            ]
        );
    }

    #[test]
    fn console_lines_keep_crlf_terminated_content() {
        // Regression: CRLF line endings must not be wiped by the CR-collapse.
        assert_eq!(
            console_lines(b"True\r\n"),
            vec!["True".to_owned(), String::new()]
        );
        assert_eq!(
            console_lines(b"Line one\r\nLine two\r\n"),
            vec!["Line one".to_owned(), "Line two".to_owned(), String::new()]
        );
    }
}
