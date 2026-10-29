//! Scripting commands against a running server: listing workspaces, typing
//! into panes, capturing screens and saving layouts.

use anyhow::Result;
use rmx_proto::Request;
use tokio::runtime::Runtime;

use super::ipc::{request_existing, simple};

pub(super) fn list(runtime: &Runtime, server: &str, json: bool) -> Result<i32> {
    let body = runtime.block_on(request_existing(server, Request::List))?;
    if json {
        println!("{body}");
    } else {
        print_list(&body)?;
    }
    Ok(0)
}

pub(super) fn send(
    runtime: &Runtime,
    server: &str,
    target: Option<String>,
    enter: bool,
    text: Vec<String>,
) -> Result<i32> {
    let mut text = text.join(" ");
    if enter {
        text.push('\r');
    }
    simple(runtime, server, Request::SendKeys { target, text })
}

pub(super) fn capture(runtime: &Runtime, server: &str, target: Option<String>) -> Result<i32> {
    let body = runtime.block_on(request_existing(server, Request::Capture { target }))?;
    let v: serde_json::Value = serde_json::from_str(&body)?;
    print!("{}", v["text"].as_str().unwrap_or_default());
    Ok(0)
}

pub(super) fn save(runtime: &Runtime, server: &str) -> Result<i32> {
    let body = runtime.block_on(request_existing(server, Request::Save))?;
    let v: serde_json::Value = serde_json::from_str(&body)?;
    println!("saved to {}", v["path"].as_str().unwrap_or("?"));
    Ok(0)
}

fn print_list(body: &str) -> Result<()> {
    let v: serde_json::Value = serde_json::from_str(body)?;
    let workspaces = v["workspaces"].as_array().cloned().unwrap_or_default();
    if workspaces.is_empty() {
        println!("no workspaces");
    }
    for w in workspaces {
        let tabs = w["tabs"].as_array().cloned().unwrap_or_default();
        let attached = w["attached"].as_u64().unwrap_or(0);
        println!(
            "{}: {} tab{}{}",
            w["name"].as_str().unwrap_or("?"),
            tabs.len(),
            if tabs.len() == 1 { "" } else { "s" },
            if attached > 0 {
                format!(" (attached: {attached})")
            } else {
                String::new()
            }
        );
        for t in tabs {
            let panes = t["panes"].as_array().map_or(0, Vec::len);
            let mark = if t["active"].as_bool() == Some(true) {
                "*"
            } else {
                " "
            };
            println!(
                "  {}{mark} {}  [{} pane{}]",
                t["index"],
                t["name"].as_str().unwrap_or("?"),
                panes,
                if panes == 1 { "" } else { "s" }
            );
        }
    }
    Ok(())
}
