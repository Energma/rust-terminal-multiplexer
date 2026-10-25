//! `config.toml`: loading, validation with precise locations, and the effective keymap.
//!
//! The document is walked as span-carrying TOML values rather than deserialised
//! with serde, so every error and warning can point at a line and column and
//! suggest a fix. Effective config = preset + user file, rebuilt on every load,
//! so deleting a line reverts that setting.

use std::collections::HashMap;
use std::fmt;
use std::ops::Range;
use std::path::{Path, PathBuf};

use ratatui::style::Color;
use serde_spanned::Spanned;
use toml::de::{DeTable, DeValue};

use crate::action::{self, Action};
use crate::diag::{Diag, did_you_mean, line_col};
use crate::keymap::{Keymap, MODES, fold_leader};
use crate::keys::{KeyChord, parse_sequence};
use crate::presets::{self, PRESETS};

/// A commented starter config (`rmx config init`). Parses with no warnings.
pub const DEFAULT_CONFIG_TOML: &str = include_str!("default_config.toml");

/// Whether the Ctrl+Shift "direct" layer is active (PLAN §5.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectKeys {
    /// On when the terminal can report Ctrl+Shift keys distinctly.
    Auto,
    On,
    Off,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct General {
    /// `None` = detect (`$SHELL`, `pwsh`, ...); `"auto"` in the file.
    pub shell: Option<String>,
    pub scrollback: usize,
    pub mouse: bool,
    pub copy_on_select: bool,
    pub restore_on_start: bool,
}

impl Default for General {
    fn default() -> Self {
        Self {
            shell: None,
            scrollback: 10_000,
            mouse: true,
            copy_on_select: true,
            restore_on_start: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaderConfig {
    pub key: KeyChord,
    /// 0 = leader mode and pending chords never time out.
    pub timeout_ms: u64,
    pub show_hints: bool,
}

impl LeaderConfig {
    fn for_preset(preset: &str) -> Self {
        Self {
            key: presets::preset_leader(preset),
            timeout_ms: 2000,
            show_hints: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub status_bg: Color,
    pub status_fg: Color,
    pub accent: Color,
    pub border: Color,
    pub active_border: Color,
    pub menu_bg: Color,
    pub menu_fg: Color,
    pub menu_highlight_bg: Color,
    pub menu_highlight_fg: Color,
}

/// Catppuccin Mocha: readable on dark terminals, and familiar to many users.
impl Default for Theme {
    fn default() -> Self {
        Self {
            status_bg: Color::Rgb(0x1e, 0x1e, 0x2e),
            status_fg: Color::Rgb(0xcd, 0xd6, 0xf4),
            accent: Color::Rgb(0xcb, 0xa6, 0xf7),
            border: Color::Rgb(0x45, 0x47, 0x5a),
            active_border: Color::Rgb(0x89, 0xb4, 0xfa),
            menu_bg: Color::Rgb(0x31, 0x32, 0x44),
            menu_fg: Color::Rgb(0xcd, 0xd6, 0xf4),
            menu_highlight_bg: Color::Rgb(0x89, 0xb4, 0xfa),
            menu_highlight_fg: Color::Rgb(0x1e, 0x1e, 0x2e),
        }
    }
}

const THEME_KEYS: &[&str] = &[
    "status_bg",
    "status_fg",
    "accent",
    "border",
    "active_border",
    "menu_bg",
    "menu_fg",
    "menu_highlight_bg",
    "menu_highlight_fg",
];

impl Theme {
    fn slot(&mut self, name: &str) -> Option<&mut Color> {
        Some(match name {
            "status_bg" => &mut self.status_bg,
            "status_fg" => &mut self.status_fg,
            "accent" => &mut self.accent,
            "border" => &mut self.border,
            "active_border" => &mut self.active_border,
            "menu_bg" => &mut self.menu_bg,
            "menu_fg" => &mut self.menu_fg,
            "menu_highlight_bg" => &mut self.menu_highlight_bg,
            "menu_highlight_fg" => &mut self.menu_highlight_fg,
            _ => return None,
        })
    }
}

/// An extra command-palette entry from `[[commands]]`.
#[derive(Debug, Clone, PartialEq)]
pub struct CustomCommand {
    pub name: String,
    pub action: Action,
}

/// One `[keys.<mode>]` entry as written; `action: None` unbinds.
#[derive(Debug, Clone, PartialEq)]
struct UserBinding {
    mode: String,
    keys: Vec<KeyChord>,
    action: Option<Action>,
    span: Range<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub preset: String,
    pub direct_keys: DirectKeys,
    pub general: General,
    pub leader: LeaderConfig,
    pub theme: Theme,
    pub commands: Vec<CustomCommand>,
    user_keys: Vec<UserBinding>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            preset: "windows".into(),
            direct_keys: DirectKeys::Auto,
            general: General::default(),
            leader: LeaderConfig::for_preset("windows"),
            theme: Theme::default(),
            commands: Vec::new(),
            user_keys: Vec::new(),
        }
    }
}

const TOP_KEYS: &[&str] = &[
    "preset",
    "direct_keys",
    "general",
    "leader",
    "theme",
    "keys",
    "commands",
];
const GENERAL_KEYS: &[&str] = &[
    "shell",
    "scrollback",
    "mouse",
    "copy_on_select",
    "restore_on_start",
];
const LEADER_KEYS: &[&str] = &["key", "timeout_ms", "show_hints"];

/// Keys Windows Terminal binds by default, so they never reach rmx there.
const WT_KEYS: &[&str] = &[
    "ctrl+shift+t",
    "ctrl+shift+w",
    "ctrl+shift+p",
    "ctrl+shift+f",
    "ctrl+shift+c",
    "ctrl+shift+v",
    "ctrl+tab",
    "ctrl+shift+tab",
    "alt+left",
    "alt+right",
    "alt+up",
    "alt+down",
    "alt+shift+left",
    "alt+shift+right",
    "alt+shift+up",
    "alt+shift+down",
    "ctrl+alt+1",
    "ctrl+alt+2",
    "ctrl+alt+3",
    "ctrl+alt+4",
    "ctrl+alt+5",
    "ctrl+alt+6",
    "ctrl+alt+7",
    "ctrl+alt+8",
    "ctrl+alt+9",
];

impl Config {
    /// `RMX_CONFIG` > Windows `%APPDATA%\rmx` > macOS `$XDG_CONFIG_HOME/rmx` or
    /// `~/.config/rmx` if it exists, else `~/Library/Application Support/rmx` >
    /// Linux `$XDG_CONFIG_HOME/rmx`. Always ends in `config.toml` (except the env override).
    pub fn default_path() -> PathBuf {
        if let Some(p) = std::env::var_os("RMX_CONFIG").filter(|p| !p.is_empty()) {
            return PathBuf::from(p);
        }
        if cfg!(target_os = "macos") {
            // Many CLI users keep dotfiles in ~/.config even on macOS; honour that first.
            let xdg = std::env::var_os("XDG_CONFIG_HOME")
                .filter(|p| !p.is_empty())
                .map(PathBuf::from)
                .or_else(|| dirs::home_dir().map(|h| h.join(".config")));
            if let Some(dir) = xdg.map(|x| x.join("rmx")).filter(|d| d.is_dir()) {
                return dir.join("config.toml");
            }
        }
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("rmx")
            .join("config.toml")
    }

    /// A missing file means defaults with no warnings.
    pub fn load(path: &Path) -> Result<(Config, Vec<Warning>), ConfigError> {
        let origin = path.display().to_string();
        match std::fs::read_to_string(path) {
            Ok(src) => Self::parse(&src, &origin),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((Config::default(), vec![])),
            Err(e) => Err(ConfigError {
                origin,
                line: 0,
                col: 0,
                message: format!("cannot read config: {e}"),
                help: None,
                snippet: None,
            }),
        }
    }

    pub fn parse(src: &str, origin: &str) -> Result<(Config, Vec<Warning>), ConfigError> {
        let root = DeTable::parse(src).map_err(|e| {
            let span = e.span().unwrap_or(0..0);
            ConfigError::from_diag(src, origin, Diag::new(span, e.message().trim()))
        })?;
        let mut p = Parser {
            src,
            config: Config::default(),
            warnings: Vec::new(),
        };
        p.document(root.get_ref())
            .map_err(|d| ConfigError::from_diag(src, origin, d))?;
        let warnings = p
            .warnings
            .into_iter()
            .map(|d| Warning::from_diag(src, origin, d))
            .collect();
        Ok((p.config, warnings))
    }

    /// Effective keymap: preset bindings with user bindings merged over them (user
    /// wins; `"none"` unbinds). `direct` includes the Ctrl+Shift layer (PLAN §5.5).
    pub fn keymap(&self, direct: bool) -> Keymap {
        Keymap::build(
            self.leader.key,
            self.leader.timeout_ms,
            presets::bindings(&self.preset, self.leader.key, direct),
            self.user_keys
                .iter()
                .map(|b| (b.mode.clone(), b.keys.clone(), b.action.clone())),
        )
    }

    pub fn direct_enabled(&self, terminal_supports_enhanced_keys: bool) -> bool {
        match self.direct_keys {
            DirectKeys::Auto => cfg!(windows) || terminal_supports_enhanced_keys,
            DirectKeys::On => true,
            DirectKeys::Off => false,
        }
    }
}

type Value<'i> = Spanned<DeValue<'i>>;
type Entry<'a, 'i> = (&'a Spanned<std::borrow::Cow<'i, str>>, &'a Value<'i>);

/// Table entries in source order (DeTable itself is sorted by key), so later
/// duplicates win and diagnostics come out top to bottom.
fn entries<'a, 'i>(t: &'a DeTable<'i>) -> Vec<Entry<'a, 'i>> {
    let mut v: Vec<_> = t.iter().collect();
    v.sort_by_key(|(k, _)| k.span().start);
    v
}

fn as_table<'a, 'i>(v: &'a Value<'i>, what: &str) -> Result<&'a DeTable<'i>, Diag> {
    v.get_ref().as_table().ok_or_else(|| {
        Diag::new(
            v.span(),
            format!("{what} must be a table, found {}", v.get_ref().type_str()),
        )
    })
}

fn as_str<'a>(v: &'a Value<'_>, name: &str) -> Result<&'a str, Diag> {
    v.get_ref().as_str().ok_or_else(|| {
        Diag::new(
            v.span(),
            format!(
                "`{name}` must be a string, found {}",
                v.get_ref().type_str()
            ),
        )
    })
}

fn as_bool(v: &Value<'_>, name: &str) -> Result<bool, Diag> {
    v.get_ref().as_bool().ok_or_else(|| {
        Diag::new(
            v.span(),
            format!(
                "`{name}` must be true or false, found {}",
                v.get_ref().type_str()
            ),
        )
    })
}

fn as_int(v: &Value<'_>, name: &str, min: i64, max: i64) -> Result<i64, Diag> {
    let Some(i) = v.get_ref().as_integer() else {
        return Err(Diag::new(
            v.span(),
            format!(
                "`{name}` must be an integer, found {}",
                v.get_ref().type_str()
            ),
        ));
    };
    match i64::from_str_radix(i.as_str(), i.radix()) {
        Ok(n) if (min..=max).contains(&n) => Ok(n),
        _ => Err(Diag::new(
            v.span(),
            format!("`{name}` must be between {min} and {max}"),
        )),
    }
}

fn unknown_key(k: &Spanned<std::borrow::Cow<'_, str>>, section: &str, known: &[&str]) -> Diag {
    let name: &str = k.get_ref();
    let msg = if section.is_empty() {
        format!("unknown setting `{name}`")
    } else {
        format!("unknown setting `{name}` in [{section}]")
    };
    let d = Diag::new(k.span(), msg);
    match did_you_mean(name, known.iter().copied()) {
        Some(s) => d.help(format!("did you mean `{s}`?")),
        None => d.help(format!("valid settings: {}", known.join(", "))),
    }
}

struct Parser<'s> {
    src: &'s str,
    config: Config,
    warnings: Vec<Diag>,
}

impl Parser<'_> {
    fn document(&mut self, root: &DeTable<'_>) -> Result<(), Diag> {
        let mut leader_key_set = false;
        let mut keys = None;
        for (k, v) in entries(root) {
            match k.get_ref().as_ref() {
                "preset" => {
                    let s = as_str(v, "preset")?;
                    if !PRESETS.contains(&s) {
                        return Err(Diag::new(v.span(), format!("unknown preset \"{s}\""))
                            .suggest(s, PRESETS.iter().copied())
                            .help_if_none(format!("presets: {}", PRESETS.join(", "))));
                    }
                    self.config.preset = s.into();
                }
                "direct_keys" => {
                    self.config.direct_keys = match v.get_ref() {
                        DeValue::Boolean(true) => DirectKeys::On,
                        DeValue::Boolean(false) => DirectKeys::Off,
                        _ => match as_str(v, "direct_keys")? {
                            "auto" => DirectKeys::Auto,
                            "on" => DirectKeys::On,
                            "off" => DirectKeys::Off,
                            s => {
                                return Err(Diag::new(
                                    v.span(),
                                    format!(
                                        "invalid direct_keys \"{s}\" (expected auto, on or off)"
                                    ),
                                )
                                .suggest(s, ["auto", "on", "off"]));
                            }
                        },
                    }
                }
                "general" => self.general(as_table(v, "[general]")?)?,
                "leader" => leader_key_set = self.leader(as_table(v, "[leader]")?)?,
                "theme" => self.theme(as_table(v, "[theme]")?)?,
                "keys" => keys = Some(as_table(v, "[keys]")?),
                "commands" => self.commands(v)?,
                _ => return Err(unknown_key(k, "", TOP_KEYS)),
            }
        }
        if !leader_key_set {
            self.config.leader.key = presets::preset_leader(&self.config.preset);
        }
        if let Some(keys) = keys {
            self.keys(keys)?;
        }
        Ok(())
    }

    fn general(&mut self, t: &DeTable<'_>) -> Result<(), Diag> {
        let g = &mut self.config.general;
        for (k, v) in entries(t) {
            match k.get_ref().as_ref() {
                "shell" => {
                    let s = as_str(v, "shell")?.trim();
                    g.shell = (!s.is_empty() && s != "auto").then(|| s.to_string());
                }
                "scrollback" => g.scrollback = as_int(v, "scrollback", 0, 10_000_000)? as usize,
                "mouse" => g.mouse = as_bool(v, "mouse")?,
                "copy_on_select" => g.copy_on_select = as_bool(v, "copy_on_select")?,
                "restore_on_start" => g.restore_on_start = as_bool(v, "restore_on_start")?,
                _ => return Err(unknown_key(k, "general", GENERAL_KEYS)),
            }
        }
        Ok(())
    }

    /// Returns whether `key` was given (otherwise the preset's leader applies).
    fn leader(&mut self, t: &DeTable<'_>) -> Result<bool, Diag> {
        let mut key_set = false;
        let l = &mut self.config.leader;
        for (k, v) in entries(t) {
            match k.get_ref().as_ref() {
                "key" => {
                    let s = as_str(v, "key")?;
                    let keys = parse_sequence(s).map_err(|e| Diag::new(v.span(), e))?;
                    let [key] = keys[..] else {
                        return Err(Diag::new(v.span(), "the leader must be a single key")
                            .help("e.g. key = \"ctrl+space\""));
                    };
                    l.key = key;
                    key_set = true;
                }
                "timeout_ms" => l.timeout_ms = as_int(v, "timeout_ms", 0, 60_000)? as u64,
                "show_hints" => l.show_hints = as_bool(v, "show_hints")?,
                _ => return Err(unknown_key(k, "leader", LEADER_KEYS)),
            }
        }
        Ok(key_set)
    }

    fn theme(&mut self, t: &DeTable<'_>) -> Result<(), Diag> {
        for (k, v) in entries(t) {
            let name: &str = k.get_ref();
            let Some(slot) = self.config.theme.slot(name) else {
                return Err(unknown_key(k, "theme", THEME_KEYS));
            };
            let s = as_str(v, name)?;
            *slot = s.parse::<Color>().map_err(|_| {
                Diag::new(v.span(), format!("invalid color \"{s}\""))
                    .help("use \"#rrggbb\", an ANSI index like \"238\", or a name like \"blue\"")
            })?;
        }
        Ok(())
    }

    fn commands(&mut self, v: &Value<'_>) -> Result<(), Diag> {
        let Some(items) = v.get_ref().as_array() else {
            return Err(Diag::new(v.span(), "`commands` must be an array of tables")
                .help("write [[commands]] sections"));
        };
        for item in items.iter() {
            let t = as_table(item, "each command")?;
            let (mut name, mut act) = (None, None);
            for (k, v) in entries(t) {
                match k.get_ref().as_ref() {
                    "name" => name = Some(as_str(v, "name")?.to_string()),
                    "action" => {
                        act = Some(action::from_value(v)?.ok_or_else(|| {
                            Diag::new(v.span(), "a command can't have action \"none\"")
                        })?)
                    }
                    _ => return Err(unknown_key(k, "[commands]", &["name", "action"])),
                }
            }
            let missing = |what| Diag::new(item.span(), format!("command is missing `{what}`"));
            self.config.commands.push(CustomCommand {
                name: name
                    .filter(|n| !n.trim().is_empty())
                    .ok_or_else(|| missing("name"))?,
                action: act.ok_or_else(|| missing("action"))?,
            });
        }
        Ok(())
    }

    fn keys(&mut self, t: &DeTable<'_>) -> Result<(), Diag> {
        let leader = self.config.leader.key;
        // (mode, keys) after normalisation and leader folding -> first occurrence.
        let mut seen: HashMap<(String, Vec<KeyChord>), Range<usize>> = HashMap::new();
        for (mode_key, mode_val) in entries(t) {
            let mode: &str = mode_key.get_ref();
            let table = as_table(mode_val, &format!("[keys.{mode}]"))?;
            if !MODES.contains(&mode)
                && let Some(s) = did_you_mean(mode, MODES.iter().copied())
            {
                self.warnings.push(
                    Diag::new(
                        mode_key.span(),
                        format!("unknown mode \"{mode}\" (treated as a new user mode)"),
                    )
                    .help(format!("did you mean \"{s}\"?")),
                );
            }
            for (k, v) in entries(table) {
                let keys = parse_sequence(k.get_ref()).map_err(|e| Diag::new(k.span(), e))?;
                let action = action::from_value(v)?;
                let folded = fold_leader(mode.to_string(), keys.clone(), leader);
                let shown = k.get_ref().to_string();
                if let Some(first) = seen.get(&folded) {
                    let what = if folded.0 == mode {
                        format!("[keys.{mode}]")
                    } else {
                        "the leader table".to_string()
                    };
                    self.warnings.push(
                        Diag::new(
                            k.span(),
                            format!("duplicate key \"{shown}\" in {what}; this one wins"),
                        )
                        .help(format!(
                            "it normalises to the same key as line {}",
                            line_col(self.src, first.start).0
                        )),
                    );
                } else {
                    seen.insert(folded, k.span());
                }
                if action.is_some() {
                    self.key_warnings(mode, &keys, &shown, k.span());
                }
                self.config.user_keys.push(UserBinding {
                    mode: mode.to_string(),
                    keys,
                    action,
                    span: k.span(),
                });
            }
        }
        self.prefix_warnings();
        Ok(())
    }

    fn key_warnings(&mut self, mode: &str, keys: &[KeyChord], shown: &str, span: Range<usize>) {
        if mode == "normal" && keys == [self.config.leader.key] {
            self.warnings.push(
                Diag::new(
                    span.clone(),
                    format!("\"{shown}\" is the leader key; binding it in normal mode disables leader mode"),
                )
                .help("bind it under [keys.leader] instead, or change [leader] key"),
            );
        }
        for chord in keys {
            let canon = chord.to_string();
            if chord.is_ctrl_shift_letter() {
                let unshifted = KeyChord {
                    code: chord.code,
                    mods: chord.mods - crossterm::event::KeyModifiers::SHIFT,
                };
                self.warnings.push(
                    Diag::new(
                        span.clone(),
                        format!("\"{canon}\" can't be told apart from \"{unshifted}\" in most terminals"),
                    )
                    .help("it works on Windows and in terminals with the kitty keyboard protocol; set direct_keys = \"on\" if yours supports it"),
                );
            }
            if WT_KEYS.contains(&canon.as_str()) {
                self.warnings.push(
                    Diag::new(
                        span.clone(),
                        format!(
                            "Windows Terminal uses \"{canon}\" itself, so rmx won't see it there"
                        ),
                    )
                    .help("`rmx doctor` prints a settings.json snippet that frees these keys"),
                );
            }
        }
    }

    /// A key that is both a binding and the start of a longer one makes the
    /// longer one unreachable (the short one fires immediately).
    fn prefix_warnings(&mut self) {
        let km = self.config.keymap(true);
        let leader = self.config.leader.key;
        for b in &self.config.user_keys {
            if b.action.is_none() {
                continue;
            }
            let (mode, keys) = fold_leader(b.mode.clone(), b.keys.clone(), leader);
            let conflict = km.bindings(&mode).iter().find(|o| {
                o.keys != keys && (o.keys.starts_with(&keys) || keys.starts_with(&o.keys))
            });
            if let Some(o) = conflict {
                let (short, long) = if o.keys.len() < keys.len() {
                    (&o.keys, &keys)
                } else {
                    (&keys, &o.keys)
                };
                let fmt = |k: &[KeyChord]| {
                    k.iter()
                        .map(|c| c.to_string())
                        .collect::<Vec<_>>()
                        .join(" ")
                };
                self.warnings.push(
                    Diag::new(
                        b.span.clone(),
                        format!(
                            "\"{}\" is bound and also starts \"{}\" in {mode} mode; the longer binding can never fire",
                            fmt(short),
                            fmt(long)
                        ),
                    )
                    .help("unbind one of them with \"none\""),
                );
            }
        }
    }
}

impl Diag {
    fn help_if_none(self, help: String) -> Self {
        if self.help.is_some() {
            self
        } else {
            self.help(help)
        }
    }
}

/// The offending source line plus a caret run under the span.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Snippet {
    text: String,
    caret_col: usize,
    caret_len: usize,
}

fn locate(src: &str, span: &Range<usize>) -> (usize, usize, Option<Box<Snippet>>) {
    let (line, col) = line_col(src, span.start);
    let text = src.lines().nth(line - 1).unwrap_or("").to_string();
    let line_len = text.chars().count();
    let span_len = src
        .get(span.clone())
        .map_or(1, |s| s.lines().next().unwrap_or("").chars().count());
    let caret_len = span_len.clamp(1, line_len.saturating_sub(col - 1).max(1));
    let snippet = Snippet {
        text,
        caret_col: col,
        caret_len,
    };
    (line, col, Some(Box::new(snippet)))
}

fn render(
    f: &mut fmt::Formatter<'_>,
    origin: &str,
    line: usize,
    col: usize,
    message: &str,
    help: Option<&str>,
    snippet: Option<&Snippet>,
) -> fmt::Result {
    if line == 0 {
        write!(f, "{origin}: {message}")?;
    } else {
        write!(f, "{origin}:{line}:{col}: {message}")?;
    }
    if let Some(h) = help {
        write!(f, "\n  help: {h}")?;
    }
    if let Some(s) = snippet.filter(|_| line > 0) {
        let gutter = line.to_string().len();
        let pad = " ".repeat(gutter);
        write!(
            f,
            "\n{pad} |\n{line} | {}\n{pad} | {}{}",
            s.text,
            " ".repeat(s.caret_col - 1),
            "^".repeat(s.caret_len)
        )?;
    }
    Ok(())
}

/// A config problem that stops loading. Display gives `file:line:col: message`,
/// an optional help line, then the source line with a caret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub origin: String,
    /// 1-based; 0 when the error has no location (e.g. unreadable file).
    pub line: usize,
    pub col: usize,
    pub message: String,
    pub help: Option<String>,
    snippet: Option<Box<Snippet>>,
}

impl ConfigError {
    fn from_diag(src: &str, origin: &str, d: Diag) -> Self {
        let (line, col, snippet) = locate(src, &d.span);
        Self {
            origin: origin.into(),
            line,
            col,
            message: d.message,
            help: d.help,
            snippet,
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        render(
            f,
            &self.origin,
            self.line,
            self.col,
            &self.message,
            self.help.as_deref(),
            self.snippet.as_deref(),
        )
    }
}

impl std::error::Error for ConfigError {}

/// A config problem that doesn't stop loading (the binding still applies).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    pub origin: String,
    pub line: usize,
    pub col: usize,
    pub message: String,
    pub help: Option<String>,
    snippet: Option<Box<Snippet>>,
}

impl Warning {
    fn from_diag(src: &str, origin: &str, d: Diag) -> Self {
        let (line, col, snippet) = locate(src, &d.span);
        Self {
            origin: origin.into(),
            line,
            col,
            message: d.message,
            help: d.help,
            snippet,
        }
    }
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        render(
            f,
            &self.origin,
            self.line,
            self.col,
            &format!("warning: {}", self.message),
            self.help.as_deref(),
            self.snippet.as_deref(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Dir;
    use crate::keymap::Source;

    fn parse(src: &str) -> (Config, Vec<Warning>) {
        Config::parse(src, "config.toml").unwrap_or_else(|e| panic!("{e}"))
    }

    fn err(src: &str) -> ConfigError {
        Config::parse(src, "config.toml").unwrap_err()
    }

    fn warns(src: &str) -> Vec<String> {
        parse(src).1.into_iter().map(|w| w.message).collect()
    }

    #[test]
    fn empty_is_default() {
        let (c, w) = parse("");
        assert_eq!(c, Config::default());
        assert!(w.is_empty());
        assert_eq!(c.leader.key, KeyChord::parse("ctrl+space").unwrap());
    }

    #[test]
    fn default_config_parses_cleanly() {
        let (c, w) = parse(DEFAULT_CONFIG_TOML);
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(c.preset, "windows");
    }

    #[test]
    fn plan_example_parses() {
        let src = r##"
preset = "windows"
direct_keys = "auto"

[general]
shell = "auto"
scrollback = 5000
mouse = false

[leader]
key = "ctrl+a"
timeout_ms = 1500

[theme]
status_bg = "#101010"
active_border = "blue"

[keys.normal]
"f10" = "open-menu"
"alt+1" = { action = "go-to-tab", index = 1 }
"ctrl+a z" = "toggle-pane-zoom"

[keys.leader]
"|" = { action = "split-pane", direction = "right" }
"-" = { action = "split-pane", direction = "down", size = "30%" }
"g" = { action = "run", command = "lazygit", in = "new-tab" }

[keys.resize]
"left" = { action = "resize-pane", direction = "left", amount = 2 }
"esc" = "exit-mode"

[[commands]]
name = "Open htop"
action = { action = "run", command = "htop", in = "split-right" }
"##;
        let (c, w) = parse(src);
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(c.general.shell, None);
        assert_eq!(c.general.scrollback, 5000);
        assert!(!c.general.mouse);
        assert_eq!(c.leader.timeout_ms, 1500);
        assert_eq!(c.theme.status_bg, Color::Rgb(0x10, 0x10, 0x10));
        assert_eq!(c.theme.active_border, Color::Blue);
        assert_eq!(c.commands.len(), 1);
        assert_eq!(c.commands[0].name, "Open htop");
        let km = c.keymap(false);
        assert_eq!(km.leader(), KeyChord::parse("ctrl+a").unwrap());
        let z = km
            .bindings("leader")
            .iter()
            .find(|b| b.keys == parse_sequence("z").unwrap())
            .unwrap();
        assert_eq!(z.source, Source::User);
    }

    #[test]
    fn preset_merge_override_and_unbind() {
        let (c, _) = parse(
            r#"
[keys.leader]
"t" = "none"
"w" = "quit"
"#,
        );
        let km = c.keymap(false);
        let leader = km.bindings("leader");
        assert!(
            !leader
                .iter()
                .any(|b| b.keys == parse_sequence("t").unwrap())
        );
        let w = leader
            .iter()
            .find(|b| b.keys == parse_sequence("w").unwrap())
            .unwrap();
        assert_eq!((w.action.clone(), w.source), (Action::Quit, Source::User));
        assert!(leader.iter().any(|b| b.source == Source::Preset));
    }

    #[test]
    fn tmux_preset_defaults_leader() {
        let (c, _) = parse("preset = \"tmux\"");
        assert_eq!(c.leader.key, KeyChord::parse("ctrl+b").unwrap());
        let km = c.keymap(true);
        assert!(
            km.bindings("leader")
                .iter()
                .any(|b| b.keys == parse_sequence("%").unwrap()
                    && b.action
                        == Action::SplitPane {
                            direction: Dir::Right,
                            size: None,
                            command: None
                        })
        );
        // tmux has no direct layer even when requested.
        assert!(
            !km.bindings("normal")
                .iter()
                .any(|b| b.keys == parse_sequence("ctrl+shift+t").unwrap())
        );
    }

    #[test]
    fn every_preset_is_clean() {
        for p in PRESETS {
            let (c, w) = parse(&format!("preset = \"{p}\""));
            assert!(w.is_empty(), "{p}: {w:?}");
            for direct in [false, true] {
                let km = c.keymap(direct);
                for mode in km.modes() {
                    let b = km.bindings(mode);
                    for (i, x) in b.iter().enumerate() {
                        for y in &b[i + 1..] {
                            assert!(
                                !x.keys.starts_with(&y.keys) && !y.keys.starts_with(&x.keys),
                                "{p}/{mode}: {:?} vs {:?}",
                                x.keys,
                                y.keys
                            );
                        }
                    }
                }
                // Escape hatches exist in every preset.
                assert!(!km.bindings("locked").is_empty(), "{p}");
                assert!(km.key_hint(&Action::OpenMenu).is_some(), "{p}");
            }
        }
    }

    #[test]
    fn unknown_action_error_has_location_and_suggestion() {
        let src = "[keys.leader]\n\"|\" = \"split-pain\"\n";
        let e = err(src);
        assert_eq!((e.line, e.col), (2, 7));
        assert_eq!(e.message, "unknown action \"split-pain\"");
        assert_eq!(e.help.as_deref(), Some("did you mean \"split-pane\"?"));
        let shown = e.to_string();
        assert!(
            shown.starts_with(
                "config.toml:2:7: unknown action \"split-pain\"\n  help: did you mean \"split-pane\"?"
            ),
            "{shown}"
        );
        assert!(
            shown.contains("2 | \"|\" = \"split-pain\"\n  |       ^^^^^^^^^^^^"),
            "{shown}"
        );
    }

    #[test]
    fn error_locations() {
        let e = err("preset = \"windows\"\nfoo = 1\n");
        assert_eq!((e.line, e.col), (2, 1));
        assert!(e.message.contains("unknown setting `foo`"));

        let e = err("[genral]\nmouse = true\n");
        assert_eq!((e.line, e.col), (1, 2));
        assert_eq!(e.help.as_deref(), Some("did you mean `general`?"));

        let e = err("[general]\nscrollbak = 5\n");
        assert_eq!((e.line, e.col), (2, 1));
        assert_eq!(e.help.as_deref(), Some("did you mean `scrollback`?"));

        let e = err("[general]\nmouse = 3\n");
        assert_eq!((e.line, e.col), (2, 9));
        assert!(e.message.contains("true or false"));

        let e = err("preset = \"tmx\"");
        assert_eq!(e.help.as_deref(), Some("did you mean \"tmux\"?"));

        let e = err("[keys.normal]\n\"ctrl+bogus\" = \"new-tab\"\n");
        assert_eq!((e.line, e.col), (2, 1));
        assert!(e.message.contains("unknown key \"bogus\""));

        let e = err(
            "[keys.leader]\nr = { action = \"enter-mode\", mode = \"resize\", stiky = true }\n",
        );
        assert_eq!((e.line, e.col), (2, 47));
        assert_eq!(e.help.as_deref(), Some("did you mean `sticky`?"));

        let e = err("[theme]\naccent = \"#zzzzzz\"\n");
        assert_eq!((e.line, e.col), (2, 10));

        let e = err("[leader]\nkey = \"ctrl+a b\"\n");
        assert!(e.message.contains("single key"));

        // TOML syntax errors keep their location too.
        let e = err("[general]\nmouse = \n");
        assert_eq!(e.line, 2);

        let e = err("[[commands]]\nname = \"x\"\n");
        assert!(e.message.contains("missing `action`"));
    }

    #[test]
    fn duplicate_after_normalisation_warns() {
        let w = warns(
            "[keys.normal]\n\"Shift+Alt+T\" = \"new-tab\"\n\"alt+shift+t\" = \"close-tab\"\n",
        );
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("duplicate key"));
        let (c, ws) = parse(
            "[keys.normal]\n\"Shift+Alt+T\" = \"new-tab\"\n\"alt+shift+t\" = \"close-tab\"\n",
        );
        assert_eq!(ws[0].line, 3);
        // Later entry wins.
        let km = c.keymap(false);
        let b = km
            .bindings("normal")
            .iter()
            .find(|b| b.keys == parse_sequence("alt+shift+t").unwrap())
            .unwrap();
        assert_eq!(b.action, Action::CloseTab);
    }

    #[test]
    fn prefix_conflict_warns() {
        let w = warns("[keys.normal]\n\"ctrl+a\" = \"new-tab\"\n\"ctrl+a x\" = \"quit\"\n");
        assert!(w.iter().any(|m| m.contains("also starts")), "{w:?}");
        // Against a preset binding too.
        let w = warns("[keys.normal]\n\"f10 x\" = \"quit\"\n");
        assert!(w.iter().any(|m| m.contains("also starts")), "{w:?}");
        // Leader sequences fold into the leader table and don't conflict.
        assert!(warns("[keys.normal]\n\"ctrl+space q\" = \"quit\"\n").is_empty());
    }

    #[test]
    fn leader_in_normal_warns() {
        let w = warns("[keys.normal]\n\"ctrl+space\" = \"new-tab\"\n");
        assert!(w.iter().any(|m| m.contains("is the leader key")), "{w:?}");
        // Fine in other modes.
        assert!(warns("[keys.select]\n\"ctrl+space\" = \"exit-mode\"\n").is_empty());
    }

    #[test]
    fn ctrl_shift_letter_warns() {
        let w = warns("[keys.normal]\n\"ctrl+shift+a\" = \"new-tab\"\n");
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("ctrl+a"), "{w:?}");
    }

    #[test]
    fn windows_terminal_keys_warn_for_user_bindings_only() {
        for k in [
            "ctrl+tab",
            "alt+left",
            "alt+shift+down",
            "ctrl+alt+3",
            "ctrl+shift+v",
        ] {
            let w = warns(&format!("[keys.normal]\n\"{k}\" = \"new-tab\"\n"));
            assert!(
                w.iter().any(|m| m.contains("Windows Terminal")),
                "{k}: {w:?}"
            );
        }
        // Unbinding is not a use; presets never warn (covered by every_preset_is_clean).
        assert!(warns("[keys.normal]\n\"ctrl+tab\" = \"none\"\n").is_empty());
    }

    #[test]
    fn mode_name_typo_warns_but_user_modes_are_fine() {
        let w = warns("[keys.resze]\nq = \"exit-mode\"\n");
        assert!(w[0].contains("unknown mode"), "{w:?}");
        assert!(warns("[keys.git]\ns = \"quit\"\n").is_empty());
    }

    #[test]
    fn warning_display_has_location() {
        let (_, w) = parse("[keys.normal]\n\"ctrl+shift+a\" = \"new-tab\"\n");
        let s = w[0].to_string();
        assert!(s.starts_with("config.toml:2:1: warning: "), "{s}");
    }

    #[test]
    fn direct_keys_setting() {
        let (c, _) = parse("direct_keys = \"off\"");
        assert!(!c.direct_enabled(true));
        let (c, _) = parse("direct_keys = \"on\"");
        assert!(c.direct_enabled(false));
        let (c, _) = parse("");
        assert!(c.direct_enabled(true));
        assert_eq!(c.direct_enabled(false), cfg!(windows));
        assert!(err("direct_keys = \"atuo\"").help.unwrap().contains("auto"));
    }

    #[test]
    fn load_missing_file_is_default() {
        let (c, w) = Config::load(Path::new("/definitely/not/here/config.toml")).unwrap();
        assert_eq!(c, Config::default());
        assert!(w.is_empty());
    }

    #[test]
    fn default_path_ends_in_config_toml() {
        // RMX_CONFIG may be set in the environment; only check the shape otherwise.
        if std::env::var_os("RMX_CONFIG").is_none() {
            let p = Config::default_path();
            assert!(p.ends_with("rmx/config.toml"), "{}", p.display());
        }
    }
}
