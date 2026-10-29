mod listener;

use std::fs;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use rmx_proto::{ClientEnd, ServerEnd};

use super::dir::{check_dir, socket_dir};
use super::lock::{spawn_lock_path, try_lock};
use super::spawn::detached_command;
use super::{Listener, connect, endpoint_at};
use crate::Endpoint;

/// A private 0700 directory, removed on drop.
pub(super) struct TempDir(pub(super) PathBuf);

impl TempDir {
    pub(super) fn new() -> Self {
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

    pub(super) fn ep(&self) -> Endpoint {
        endpoint_at(self.0.join("t.sock"))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub(super) async fn pair(ep: &Endpoint) -> (Listener, ServerEnd, ClientEnd) {
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
