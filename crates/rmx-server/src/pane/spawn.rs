//! Starting a pane: open the PTY, launch the program and its I/O threads.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::mpsc as std_mpsc;
use std::thread;

use anyhow::{Context, Result};
use portable_pty::{CommandBuilder, native_pty_system};
use rmx_core::emulator::Vt100Emulator;
use tokio::sync::mpsc;

use super::platform::{command_argv, default_shell, dirs_home, program_name, pty_size};
use super::{Pane, PaneEvent, PaneId, SpawnSpec};

impl Pane {
    /// Spawns a pane sized `rows`×`cols`.
    pub fn spawn(
        id: PaneId,
        spec: &SpawnSpec,
        rows: u16,
        cols: u16,
        scrollback: usize,
        events: mpsc::Sender<PaneEvent>,
    ) -> Result<Self> {
        // portable-pty panics (rather than erroring) when ConPTY is missing on
        // Windows older than 10 1809; turn that into a message users can act on.
        let opened = std::panic::catch_unwind(|| native_pty_system().openpty(pty_size(rows, cols)));
        let pair = match opened {
            Ok(result) => result.context("opening a pseudo-terminal")?,
            Err(_) => anyhow::bail!(
                "this system has no pseudo-console support (Windows 10 1809 or newer is required)"
            ),
        };

        let shell = spec.shell.clone().or_else(default_shell);
        let program = program_name(spec.command.as_deref().or(shell.as_deref()));
        let mut cmd = match (&spec.command, shell) {
            (Some(command), _) => {
                let mut argv = command_argv(command).into_iter();
                let mut cmd = CommandBuilder::new(argv.next().unwrap_or_default());
                cmd.args(argv);
                cmd
            }
            (None, Some(program)) => CommandBuilder::new(program),
            (None, None) => CommandBuilder::new_default_prog(),
        };
        let cwd = spec
            .cwd
            .clone()
            .filter(|d| d.is_dir())
            .or_else(|| std::env::current_dir().ok())
            .or_else(dirs_home)
            .unwrap_or_else(|| PathBuf::from("."));
        cmd.cwd(&cwd);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("RMX", "1");
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }
        cmd.env_remove("TMUX");

        let mut child = pair
            .slave
            .spawn_command(cmd)
            .context("starting the shell")?;
        let pid = child.process_id();
        // The child holds its own handle; keeping ours open would stop the
        // reader from ever seeing EOF on Unix.
        drop(pair.slave);

        let killer = child.clone_killer();
        let mut reader = pair.master.try_clone_reader()?;
        let mut writer = pair.master.take_writer()?;

        let out_tx = events.clone();
        thread::Builder::new()
            .name(format!("pane-{id}-reader"))
            .spawn(move || {
                let mut buf = vec![0u8; 64 * 1024];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        // blocking_send applies back-pressure to the child
                        // when the core falls behind.
                        Ok(n) => {
                            if out_tx
                                .blocking_send(PaneEvent::Output(id, buf[..n].to_vec()))
                                .is_err()
                            {
                                break;
                            }
                        }
                    }
                }
                let _ = out_tx.blocking_send(PaneEvent::Closed(id));
            })?;

        let (writer_tx, writer_rx) = std_mpsc::channel::<Vec<u8>>();
        thread::Builder::new()
            .name(format!("pane-{id}-writer"))
            .spawn(move || {
                for bytes in writer_rx {
                    if writer
                        .write_all(&bytes)
                        .and_then(|()| writer.flush())
                        .is_err()
                    {
                        break;
                    }
                }
            })?;

        // Reaps the child so it doesn't linger as a zombie on Unix. The exit
        // itself is reported via the reader's EOF.
        thread::Builder::new()
            .name(format!("pane-{id}-waiter"))
            .spawn(move || {
                let status = child.wait();
                tracing::debug!(pane = id, ?status, "child exited");
            })?;

        Ok(Self {
            id,
            emulator: Box::new(Vt100Emulator::new(rows, cols, scrollback)),
            command: spec.command.clone(),
            program,
            size: (rows, cols),
            spawn_cwd: cwd,
            pid,
            master: Some(pair.master),
            writer_tx: Some(writer_tx),
            killer,
        })
    }
}
