use std::fs;

use sha2::{Digest, Sha256};

use super::install::{replace, verify};
use super::release::{asset_name, latest_tag};

#[test]
fn asset_names_follow_the_release_workflow() {
    assert_eq!(
        asset_name("v0.2.0", "x86_64-unknown-linux-musl"),
        "rmx-v0.2.0-x86_64-unknown-linux-musl.tar.gz"
    );
    assert_eq!(
        asset_name("v0.2.0", "x86_64-pc-windows-msvc"),
        "rmx-v0.2.0-x86_64-pc-windows-msvc.zip"
    );
}

#[test]
fn latest_tag_reads_github_json() {
    assert_eq!(
        latest_tag(r#"{"tag_name":"v0.3.1","assets":[]}"#).unwrap(),
        "v0.3.1"
    );
    assert!(latest_tag(r#"{"message":"Not Found"}"#).is_err());
    assert!(latest_tag("rmx: nope").is_err());
}

#[test]
fn verify_matches_shasum_output() {
    let data = b"payload";
    let hex = Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    assert!(verify(data, &format!("{hex}  rmx-v1.tar.gz\n")).is_ok());
    assert!(verify(b"tampered", &format!("{hex}  rmx-v1.tar.gz\n")).is_err());
    assert!(verify(data, "").is_err());
}

#[test]
fn replace_swaps_the_file_and_leaves_no_temp() {
    let dir = std::env::temp_dir().join(format!("rmx-replace-test-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let dest = dir.join("rmx");
    fs::write(&dest, b"old").unwrap();
    replace(&dest, b"new").unwrap();
    assert_eq!(fs::read(&dest).unwrap(), b"new");
    assert!(!dir.join("rmx.new").exists());
    fs::remove_dir_all(&dir).unwrap();
}
