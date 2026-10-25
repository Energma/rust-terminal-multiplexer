//! Unix domain sockets in a private per-user directory.
//!
//! Security rests on the directory: it must be ours, 0700 and not a symlink,
//! so nobody else can create, replace or connect to the socket. The socket is
//! additionally 0600 and every peer's uid is checked, in case the directory
//! check is ever bypassed (e.g. by a careless chmod). We never use the Linux
//! abstract namespace, which has no permissions at all.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{Context, bail};
use rmx_proto::{ClientEnd, ServerEnd};
use tokio::net::{UnixListener, UnixStream};
use tokio::time::Instant;

use crate::{Endpoint, SpawnInfo, client_end, server_end};

/// Builds an endpoint at an explicit socket path (tests, `--socket` style overrides).
pub fn endpoint_at(path: impl Into<PathBuf>) -> Endpoint {
    Endpoint { path: path.into() }
}

fn euid() -> u32 {
    // SAFETY: geteuid has no preconditions and cannot fail.
    unsafe { libc::geteuid() }
}

pub(crate) fn endpoint(name: &str) -> anyhow::Result<Endpoint> {
    let dir = socket_dir(
        std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from),
        std::env::var_os("TMPDIR").map(PathBuf::from),
        euid(),
    );
    Ok(endpoint_at(dir.join(format!("{name}.sock"))))
}

/// Relative env values are ignored: they'd resolve against whatever folder
/// the client happened to start in, so two clients wouldn't find each other.
fn socket_dir(xdg: Option<PathBuf>, tmp: Option<PathBuf>, uid: u32) -> PathBuf {
    if let Some(x) = xdg.filter(|p| p.is_absolute()) {
        return x.join("rmx");
    }
    let base = tmp
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    base.join(format!("rmx-{uid}"))
}

fn parent(ep: &Endpoint) -> anyhow::Result<&Path> {
    ep.path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .with_context(|| format!("socket path {} has no directory", ep.path.display()))
}

/// Creates `dir` (one level, 0700) if missing, then insists it is safe.
fn ensure_dir(dir: &Path) -> anyhow::Result<()> {
    match fs::DirBuilder::new().mode(0o700).create(dir) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e).with_context(|| format!("creating {}", dir.display())),
    }
    check_dir(dir)
}

/// The directory must be a real directory (not a symlink), owned by us, with
/// no group/other access. Anything else means someone else could plant or
/// hijack our socket, so refuse rather than "fix" it.
pub(crate) fn check_dir(dir: &Path) -> anyhow::Result<()> {
    let meta =
        fs::symlink_metadata(dir).with_context(|| format!("inspecting {}", dir.display()))?;
    if meta.file_type().is_symlink() {
        bail!(
            "{} is a symlink; refusing to use it for the rmx socket",
            dir.display()
        );
    }
    if !meta.is_dir() {
        bail!("{} is not a directory", dir.display());
    }
    if meta.uid() != euid() {
        bail!(
            "{} is owned by uid {}, not us (uid {}); refusing to use it",
            dir.display(),
            meta.uid(),
            euid()
        );
    }
    let mode = meta.mode() & 0o777;
    if mode & 0o077 != 0 {
        bail!(
            "{} has mode {mode:o}; it must be 0700 (run `chmod 700 {}`)",
            dir.display(),
            dir.display()
        );
    }
    Ok(())
}

/// Opens (creating 0600) a lock file and tries to take an exclusive `flock`.
/// Returns `Ok(None)` if someone else holds it. The lock lives as long as the
/// returned `File`; std opens with `O_CLOEXEC`, so pane shells never inherit it.
fn try_lock(path: &Path) -> anyhow::Result<Option<File>> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .with_context(|| format!("opening lock file {}", path.display()))?;
    // SAFETY: the fd is valid for the duration of the call.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        return Ok(Some(file));
    }
    let err = io::Error::last_os_error();
    if err.kind() == io::ErrorKind::WouldBlock {
        Ok(None)
    } else {
        Err(err).with_context(|| format!("locking {}", path.display()))
    }
}

/// Held by the server for its whole life. It makes "is a server already
/// running?" race-free, so removing a stale socket can never delete the
/// fresh one of a server that is starting at the same moment.
fn server_lock_path(ep: &Endpoint) -> PathBuf {
    ep.path.with_extension("lock")
}

/// Held by a client between "nobody answers" and "the server it spawned
/// answers". Separate from the server lock, which the spawned server needs
/// while this one is still held.
/// A distinct extension (not `spawn.lock`) so the spawn lock of server `x`
/// can never be the server lock of a server named `x.spawn`.
fn spawn_lock_path(ep: &Endpoint) -> PathBuf {
    ep.path.with_extension("spawnlock")
}

pub struct Listener {
    inner: UnixListener,
    path: PathBuf,
    _lock: File,
}

impl Listener {
    /// Creates the endpoint securely and errors clearly if a live server
    /// already listens there.
    pub async fn bind(ep: &Endpoint) -> anyhow::Result<Listener> {
        ensure_dir(parent(ep)?)?;
        let already = || anyhow::anyhow!("an rmx server is already running on {}", ep.display());
        let lock = try_lock(&server_lock_path(ep))?.ok_or_else(already)?;

        match fs::symlink_metadata(&ep.path) {
            Ok(meta) => {
                // Lock is free, so this should be a leftover from a crash —
                // but double-check before deleting anything.
                if is_alive(ep).await {
                    return Err(already());
                }
                if !meta.file_type().is_socket() {
                    bail!(
                        "{} exists and is not a socket; refusing to remove it",
                        ep.display()
                    );
                }
                fs::remove_file(&ep.path)
                    .with_context(|| format!("removing stale socket {}", ep.display()))?;
                tracing::info!("removed stale socket {}", ep.display());
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).with_context(|| format!("inspecting {}", ep.display())),
        }

        let inner =
            UnixListener::bind(&ep.path).with_context(|| format!("binding {}", ep.display()))?;
        // chmod after bind rather than a umask dance (umask is process-wide
        // and racy with other threads); the 0700 directory covers the gap.
        fs::set_permissions(&ep.path, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("restricting {}", ep.display()))?;
        Ok(Listener {
            inner,
            path: ep.path.clone(),
            _lock: lock,
        })
    }

    /// Waits for the next client whose uid matches ours; others are logged
    /// and dropped.
    pub async fn accept(&mut self) -> anyhow::Result<ServerEnd> {
        loop {
            let (stream, _) = self.inner.accept().await.context("accepting a client")?;
            match stream.peer_cred() {
                Ok(cred) if cred.uid() == euid() => return Ok(server_end(stream)),
                Ok(cred) => tracing::warn!("rejected client with uid {}", cred.uid()),
                Err(e) => tracing::warn!("rejected client: cannot read its credentials: {e}"),
            }
        }
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        // Safe to remove unconditionally: we still hold the server lock, so
        // nobody else can have bound this path since.
        let _ = fs::remove_file(&self.path);
    }
}

pub(crate) async fn connect(ep: &Endpoint) -> anyhow::Result<ClientEnd> {
    // Don't talk to a socket in a directory someone else could control.
    check_dir(parent(ep)?)?;
    let stream = UnixStream::connect(&ep.path)
        .await
        .with_context(|| format!("connecting to {}", ep.display()))?;
    let cred = stream.peer_cred().context("reading server credentials")?;
    if cred.uid() != euid() {
        bail!(
            "server on {} runs as uid {}, not us; refusing to connect",
            ep.display(),
            cred.uid()
        );
    }
    Ok(client_end(stream))
}

pub(crate) async fn is_alive(ep: &Endpoint) -> bool {
    // A stale socket refuses immediately; the timeout only guards against a
    // wedged server whose backlog is full.
    matches!(
        tokio::time::timeout(Duration::from_secs(1), UnixStream::connect(&ep.path)).await,
        Ok(Ok(_))
    )
}

pub(crate) async fn spawn_lock(ep: &Endpoint) -> anyhow::Result<File> {
    ensure_dir(parent(ep)?)?;
    let path = spawn_lock_path(ep);
    // Generous: the holder may itself be waiting ~3 s for its server.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(f) = try_lock(&path)? {
            return Ok(f);
        }
        if Instant::now() >= deadline {
            bail!("timed out waiting for {}", path.display());
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Split out of [`spawn_detached`] so tests can inspect it without spawning.
pub(crate) fn detached_command(exe: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(exe);
    cmd.args(args)
        .current_dir(crate::neutral_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: the closure runs between fork and exec and only calls
    // async-signal-safe functions (setsid, signal).
    unsafe {
        cmd.pre_exec(|| {
            // New session: no controlling terminal, so closing the terminal
            // doesn't SIGHUP the server.
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            // Ignored dispositions survive exec; belt and braces for SIGHUP,
            // and a dead client socket must be an EPIPE, not a kill.
            libc::signal(libc::SIGHUP, libc::SIG_IGN);
            libc::signal(libc::SIGPIPE, libc::SIG_IGN);
            Ok(())
        });
    }
    cmd
}

pub(crate) fn spawn_detached(exe: &Path, args: &[&str]) -> anyhow::Result<SpawnInfo> {
    let mut child = detached_command(exe, args)
        .spawn()
        .with_context(|| format!("starting {}", exe.display()))?;
    let pid = child.id();
    // Reap it if it exits while we're still running, so it doesn't linger as
    // a zombie; if we exit first, init adopts it.
    std::thread::Builder::new()
        .name("rmx-reaper".into())
        .spawn(move || {
            let _ = child.wait();
        })
        .context("spawning reaper thread")?;
    Ok(SpawnInfo {
        pid,
        breakaway: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmx_proto::{ClientMsg, ServerMsg};
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A private 0700 directory, removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static N: AtomicU32 = AtomicU32::new(0);
            // Short base: sun_path is only ~104-108 bytes.
            let p = PathBuf::from("/tmp").join(format!(
                "rmx-ipc-test-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&p);
            fs::DirBuilder::new().mode(0o700).create(&p).unwrap();
            Self(p)
        }

        fn ep(&self) -> Endpoint {
            endpoint_at(self.0.join("t.sock"))
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    async fn pair(ep: &Endpoint) -> (Listener, ServerEnd, ClientEnd) {
        let mut l = Listener::bind(ep).await.unwrap();
        let (client, server) = tokio::join!(connect(ep), l.accept());
        (l, server.unwrap(), client.unwrap())
    }

    #[test]
    fn socket_dir_prefers_xdg_then_tmpdir_then_tmp() {
        let xdg = Some(PathBuf::from("/run/user/1000"));
        let tmp = Some(PathBuf::from("/var/tmp"));
        assert_eq!(
            socket_dir(xdg, tmp.clone(), 7),
            PathBuf::from("/run/user/1000/rmx")
        );
        assert_eq!(
            socket_dir(Some("rel".into()), tmp, 7),
            PathBuf::from("/var/tmp/rmx-7")
        );
        assert_eq!(socket_dir(None, None, 7), PathBuf::from("/tmp/rmx-7"));
    }

    #[test]
    fn endpoint_puts_name_in_socket_file() {
        let ep = crate::endpoint("work").unwrap();
        assert_eq!(ep.path().file_name().unwrap(), "work.sock");
    }

    #[tokio::test]
    async fn round_trips_messages_both_ways() {
        let dir = TempDir::new();
        let (_l, mut server, client) = pair(&dir.ep()).await;
        let ClientEnd { tx, mut rx } = client;
        for _ in 0..3 {
            tx.send(ClientMsg::Detach).await.unwrap();
        }
        tx.send(ClientMsg::Paste("hi".into())).await.unwrap();
        for _ in 0..3 {
            assert_eq!(server.rx.recv().await, Some(ClientMsg::Detach));
        }
        assert_eq!(server.rx.recv().await, Some(ClientMsg::Paste("hi".into())));
        for _ in 0..20 {
            server.tx.send(ServerMsg::Bell).await.unwrap();
            assert_eq!(rx.recv().await, Some(ServerMsg::Bell));
        }
    }

    #[tokio::test]
    async fn carries_large_messages() {
        let dir = TempDir::new();
        let (_l, mut server, client) = pair(&dir.ep()).await;
        let big = "x".repeat(1024 * 1024 + 17);
        client.tx.send(ClientMsg::Paste(big.clone())).await.unwrap();
        assert_eq!(server.rx.recv().await, Some(ClientMsg::Paste(big)));
    }

    #[tokio::test]
    async fn closing_client_ends_server_rx() {
        let dir = TempDir::new();
        let (_l, mut server, client) = pair(&dir.ep()).await;
        drop(client);
        assert_eq!(server.rx.recv().await, None);
    }

    #[tokio::test]
    async fn dropping_server_tx_ends_client_rx_but_not_its_tx() {
        let dir = TempDir::new();
        let (_l, mut server, mut client) = pair(&dir.ep()).await;
        server.tx.send(ServerMsg::Bell).await.unwrap();
        drop(server.tx);
        assert_eq!(client.rx.recv().await, Some(ServerMsg::Bell));
        assert_eq!(client.rx.recv().await, None);
        // Half-close: the other direction keeps working.
        client.tx.send(ClientMsg::Detach).await.unwrap();
        assert_eq!(server.rx.recv().await, Some(ClientMsg::Detach));
    }

    #[test]
    fn check_dir_rejects_unsafe_directories() {
        let dir = TempDir::new();
        check_dir(&dir.0).unwrap();

        fs::set_permissions(&dir.0, fs::Permissions::from_mode(0o755)).unwrap();
        let err = check_dir(&dir.0).unwrap_err().to_string();
        assert!(err.contains("755"), "{err}");
        fs::set_permissions(&dir.0, fs::Permissions::from_mode(0o700)).unwrap();

        let link = dir.0.join("link");
        let real = dir.0.join("real");
        fs::DirBuilder::new().mode(0o700).create(&real).unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert!(
            check_dir(&link)
                .unwrap_err()
                .to_string()
                .contains("symlink")
        );
    }

    #[tokio::test]
    async fn bind_refuses_an_unsafe_directory() {
        let dir = TempDir::new();
        fs::set_permissions(&dir.0, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(Listener::bind(&dir.ep()).await.is_err());
        assert!(connect(&dir.ep()).await.is_err());
    }

    #[tokio::test]
    async fn socket_is_private() {
        let dir = TempDir::new();
        let ep = dir.ep();
        let _l = Listener::bind(&ep).await.unwrap();
        let mode = fs::metadata(ep.path()).unwrap().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[tokio::test]
    async fn replaces_a_stale_socket() {
        let dir = TempDir::new();
        let ep = dir.ep();
        // std's listener doesn't unlink on drop: exactly what a crash leaves.
        drop(std::os::unix::net::UnixListener::bind(ep.path()).unwrap());
        assert!(ep.path().exists());
        assert!(!is_alive(&ep).await);
        let (_l, mut server, client) = pair(&ep).await;
        client.tx.send(ClientMsg::Detach).await.unwrap();
        assert_eq!(server.rx.recv().await, Some(ClientMsg::Detach));
    }

    #[tokio::test]
    async fn refuses_to_remove_a_non_socket() {
        let dir = TempDir::new();
        let ep = dir.ep();
        fs::write(ep.path(), b"precious").unwrap();
        assert!(Listener::bind(&ep).await.is_err());
        assert_eq!(fs::read(ep.path()).unwrap(), b"precious");
    }

    #[tokio::test]
    async fn second_bind_on_live_endpoint_errors() {
        let dir = TempDir::new();
        let ep = dir.ep();
        let _l = Listener::bind(&ep).await.unwrap();
        let err = Listener::bind(&ep).await.err().unwrap().to_string();
        assert!(err.contains("already running"), "{err}");
        // And the failed attempt must not have removed the live socket.
        assert!(is_alive(&ep).await);
    }

    #[tokio::test]
    async fn is_alive_tracks_the_listener() {
        let dir = TempDir::new();
        let ep = dir.ep();
        assert!(!is_alive(&ep).await);
        let l = Listener::bind(&ep).await.unwrap();
        assert!(is_alive(&ep).await);
        drop(l);
        assert!(!ep.path().exists());
        assert!(!is_alive(&ep).await);
        // Lock released with the listener: a new server can start.
        let _l = Listener::bind(&ep).await.unwrap();
    }

    #[tokio::test]
    async fn connect_or_spawn_uses_a_running_server() {
        let dir = TempDir::new();
        let ep = dir.ep();
        let mut l = Listener::bind(&ep).await.unwrap();
        let (client, server) = tokio::join!(crate::connect_or_spawn(&ep, &["__never"]), l.accept());
        let (client, spawned) = client.unwrap();
        assert!(spawned.is_none());
        client.tx.send(ClientMsg::Detach).await.unwrap();
        assert_eq!(server.unwrap().rx.recv().await, Some(ClientMsg::Detach));
    }

    #[test]
    fn spawn_lock_is_exclusive() {
        let dir = TempDir::new();
        let path = spawn_lock_path(&dir.ep());
        let held = try_lock(&path).unwrap().unwrap();
        assert!(try_lock(&path).unwrap().is_none());
        drop(held);
        assert!(try_lock(&path).unwrap().is_some());
    }

    #[test]
    fn detached_command_runs_exe_with_args() {
        let cmd = detached_command(Path::new("/usr/bin/rmx"), &["__server", "-L", "x"]);
        assert_eq!(cmd.get_program(), "/usr/bin/rmx");
        let args: Vec<_> = cmd.get_args().collect();
        assert_eq!(args, ["__server", "-L", "x"]);
    }

    #[test]
    fn detached_command_pre_exec_succeeds() {
        // Spawns /bin/sh rather than current_exe (the test binary): this
        // proves the setsid/signal hook doesn't make spawning fail.
        let status = detached_command(Path::new("/bin/sh"), &["-c", "exit 3"])
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(3));
    }
}
