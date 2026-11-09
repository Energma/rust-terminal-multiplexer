use super::helpers::{project_root, unescape};

#[test]
fn unescapes_send_keys() {
    assert_eq!(unescape(r"ls\r\tx\\y\e"), "ls\r\tx\\y\x1b");
}

#[test]
fn project_root_finds_the_enclosing_repository() {
    let base = std::env::temp_dir().join(format!("rmx-project-root-{}", std::process::id()));
    let repo = base.join("repo");
    let nested = repo.join("src/deep");
    let plain = base.join("plain");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::create_dir_all(&plain).unwrap();

    assert_eq!(project_root(&nested), repo);
    assert_eq!(project_root(&repo), repo);
    assert_eq!(project_root(&plain), plain);
    // Worktrees and submodules have a `.git` file instead of a directory.
    let worktree = base.join("worktree");
    std::fs::create_dir_all(&worktree).unwrap();
    std::fs::write(worktree.join(".git"), "gitdir: elsewhere").unwrap();
    assert_eq!(project_root(&worktree), worktree);

    std::fs::remove_dir_all(&base).unwrap();
}
