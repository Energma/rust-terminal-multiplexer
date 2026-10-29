use clap::Parser;

use super::args::{Cli, Command};
use super::server::server_args;
use super::session::shell_join;

#[test]
fn shell_join_keeps_argument_boundaries() {
    let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(shell_join(&args(&["make && run"])), "make && run");
    if cfg!(windows) {
        assert_eq!(
            shell_join(&args(&["vim", "my file.txt"])),
            "vim \"my file.txt\""
        );
    } else {
        assert_eq!(
            shell_join(&args(&["sh", "-c", "echo \"a b\""])),
            "sh -c 'echo \"a b\"'"
        );
        assert_eq!(shell_join(&args(&["echo", "it's"])), "echo 'it'\\''s'");
    }
}

#[test]
fn server_args_survive_leading_dashes() {
    let cli = Cli::try_parse_from(
        std::iter::once("rmx".to_owned())
            .chain(server_args("-odd", std::path::Path::new("/tmp/-x.toml")).unwrap()),
    )
    .unwrap();
    assert_eq!(cli.server, "-odd");
    assert_eq!(
        cli.config.unwrap(),
        std::path::PathBuf::from("/tmp/-x.toml")
    );
    assert!(matches!(cli.command, Some(Command::Server { .. })));
}
