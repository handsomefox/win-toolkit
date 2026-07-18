//! The background worker: a single thread that launches elevated operations and
//! tails their captured output, talking to the UI over channels. The UI thread
//! never blocks on a running operation.

use std::path::PathBuf;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use eframe::egui;
use toolkit_core::{Operation, console_lines, elevated_cmd_parameters};
use toolkit_platform::{ExecError, run_elevated};

/// How often the worker re-reads the output file while an operation runs.
const POLL_INTERVAL: Duration = Duration::from_millis(250);

pub(crate) enum Command {
    /// Run an operation, capturing its output.
    Run(Operation),
}

pub(crate) enum Event {
    /// The operation launched and is now running (the UAC prompt was accepted).
    Started,
    /// The latest full set of output lines captured so far (replaces the
    /// previous set in the UI).
    Output { lines: Vec<String> },
    /// The operation finished (or could not be launched).
    Finished(Outcome),
}

/// How a run ended.
pub(crate) enum Outcome {
    /// The command exited with status zero.
    Success,
    /// The command exited with a non-zero status.
    Failed(i32),
    /// The user dismissed the UAC prompt.
    Declined,
    /// The operation could not be launched or polled.
    Error(String),
}

pub(crate) struct Worker {
    commands: Sender<Command>,
    events: Receiver<Event>,
}

impl Worker {
    pub(crate) fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }

    pub(crate) fn try_recv(&self) -> Option<Event> {
        self.events.try_recv().ok()
    }
}

/// Spawns the worker thread. It holds a clone of the egui context so every event
/// is followed by a repaint request.
pub(crate) fn spawn(ctx: egui::Context) -> Worker {
    let (command_tx, command_rx) = crossbeam_channel::unbounded::<Command>();
    let (event_tx, event_rx) = crossbeam_channel::unbounded::<Event>();

    std::thread::Builder::new()
        .name("toolkit-worker".to_owned())
        .spawn(move || run(&ctx, &command_rx, &event_tx))
        .expect("failed to spawn worker thread");

    Worker {
        commands: command_tx,
        events: event_rx,
    }
}

fn run(ctx: &egui::Context, commands: &Receiver<Command>, events: &Sender<Event>) {
    let emit = |event: Event| {
        let _ = events.send(event);
        ctx.request_repaint();
    };
    while let Ok(command) = commands.recv() {
        match command {
            Command::Run(operation) => run_operation(&operation, &emit),
        }
    }
}

fn run_operation(operation: &Operation, emit: &impl Fn(Event)) {
    let Some(output_path) = output_path(operation.id) else {
        emit(Event::Finished(Outcome::Error(
            "could not resolve the output directory".to_owned(),
        )));
        return;
    };
    if let Some(parent) = output_path.parent()
        && let Err(err) = std::fs::create_dir_all(parent)
    {
        emit(Event::Finished(Outcome::Error(format!(
            "could not create the output directory: {err}"
        ))));
        return;
    }

    let parameters = elevated_cmd_parameters(&operation.command, &output_path.to_string_lossy());
    tracing::info!(operation = operation.id, "launching elevated operation");
    let child = match run_elevated("cmd.exe", &parameters) {
        Ok(child) => child,
        Err(ExecError::Declined) => {
            tracing::info!(operation = operation.id, "elevation declined");
            emit(Event::Finished(Outcome::Declined));
            return;
        }
        Err(err) => {
            tracing::warn!(operation = operation.id, "launch failed: {err}");
            emit(Event::Finished(Outcome::Error(err.to_string())));
            return;
        }
    };

    emit(Event::Started);

    loop {
        std::thread::sleep(POLL_INTERVAL);
        emit(Event::Output {
            lines: read_output(&output_path),
        });
        match child.try_exit_code() {
            Ok(Some(code)) => {
                // A final read captures anything written between the last poll
                // and exit.
                emit(Event::Output {
                    lines: read_output(&output_path),
                });
                tracing::info!(operation = operation.id, code, "operation finished");
                emit(Event::Finished(if code == 0 {
                    Outcome::Success
                } else {
                    Outcome::Failed(code)
                }));
                return;
            }
            Ok(None) => {}
            Err(err) => {
                tracing::warn!(operation = operation.id, "polling failed: {err}");
                emit(Event::Finished(Outcome::Error(err.to_string())));
                return;
            }
        }
    }
}

fn output_path(operation_id: &str) -> Option<PathBuf> {
    let stamp = jiff::Zoned::now().strftime("%Y%m%d-%H%M%S").to_string();
    toolkit_platform::runs_dir().map(|dir| dir.join(format!("{operation_id}-{stamp}.log")))
}

fn read_output(path: &std::path::Path) -> Vec<String> {
    match std::fs::read(path) {
        Ok(bytes) => console_lines(&bytes),
        // The file may not exist yet for the first poll; treat as empty.
        Err(_) => Vec::new(),
    }
}
