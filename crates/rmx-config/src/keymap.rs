//! Effective key tables and the per-client key engine that walks them.
//!
//! The engine is a small state machine (mode, sticky flag, chord buffer) with
//! no I/O and an injected clock, so timeouts are testable without sleeping.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use crossterm::event::{KeyEvent, KeyEventKind};

use crate::action::Action;
use crate::keys::{KeyChord, display_sequence};

/// Built-in modes. Users may define extra ones under `[keys.<name>]`.
pub const MODES: &[&str] = &["normal", "leader", "select", "resize", "locked"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Preset,
    User,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Binding {
    pub keys: Vec<KeyChord>,
    pub action: Action,
    pub source: Source,
}

#[derive(Debug, Clone)]
pub struct Keymap {
    tables: BTreeMap<String, Vec<Binding>>,
    leader: KeyChord,
    timeout_ms: u64,
}

impl Keymap {
    /// Merges `user` over `preset`: a user entry replaces any binding with the same
    /// keys in that mode, and `None` (`"none"` in TOML) just removes it.
    ///
    /// Normal-mode sequences that start with the leader key (`"ctrl+space z"`) are
    /// folded into the leader table (`"z"`), since the engine consumes the leader
    /// key by entering leader mode.
    pub(crate) fn build(
        leader: KeyChord,
        timeout_ms: u64,
        preset: impl IntoIterator<Item = (String, Vec<KeyChord>, Action)>,
        user: impl IntoIterator<Item = (String, Vec<KeyChord>, Option<Action>)>,
    ) -> Self {
        let mut tables: BTreeMap<String, Vec<Binding>> = BTreeMap::new();
        let preset = preset
            .into_iter()
            .map(|(m, k, a)| (m, k, Some(a), Source::Preset));
        let user = user.into_iter().map(|(m, k, a)| (m, k, a, Source::User));
        for (mode, keys, action, source) in preset.chain(user) {
            let (mode, keys) = fold_leader(mode, keys, leader);
            let table = tables.entry(mode).or_default();
            table.retain(|b| b.keys != keys);
            if let Some(action) = action {
                table.push(Binding {
                    keys,
                    action,
                    source,
                });
            }
        }
        Self {
            tables,
            leader,
            timeout_ms,
        }
    }

    pub fn bindings(&self, mode: &str) -> &[Binding] {
        self.tables.get(mode).map_or(&[], Vec::as_slice)
    }

    /// Built-in modes first, then user-defined ones.
    pub fn modes(&self) -> Vec<&str> {
        let mut v: Vec<&str> = MODES.to_vec();
        v.extend(
            self.tables
                .keys()
                .map(String::as_str)
                .filter(|m| !MODES.contains(m)),
        );
        v
    }

    pub fn leader(&self) -> KeyChord {
        self.leader
    }

    /// `None` when chords and leader mode never time out.
    pub fn timeout(&self) -> Option<Duration> {
        (self.timeout_ms > 0).then(|| Duration::from_millis(self.timeout_ms))
    }

    /// Shortest binding for `action` reachable from normal mode, for UI hints:
    /// `"F10"`, `"^Space |"`. Ties prefer direct (normal-mode) keys over leader ones.
    pub fn key_hint(&self, action: &Action) -> Option<String> {
        let direct = self
            .bindings("normal")
            .iter()
            .filter(|b| b.action == *action)
            .map(|b| b.keys.clone());
        let via_leader = self
            .bindings("leader")
            .iter()
            .filter(|b| b.action == *action)
            .map(|b| {
                let mut k = vec![self.leader];
                k.extend(&b.keys);
                k
            });
        // min_by_key keeps the first minimum, so normal-mode keys win ties.
        direct
            .chain(via_leader)
            .min_by_key(Vec::len)
            .map(|k| display_sequence(&k))
    }

    /// Like [`key_hint`](Self::key_hint) but for one mode's own table (e.g. the
    /// select-mode status bar).
    pub fn key_hint_in(&self, mode: &str, action: &Action) -> Option<String> {
        self.bindings(mode)
            .iter()
            .filter(|b| b.action == *action)
            .min_by_key(|b| b.keys.len())
            .map(|b| display_sequence(&b.keys))
    }

    /// Continuations of `prefix` in `mode`: (remaining keys, action label).
    pub fn hints(&self, mode: &str, prefix: &[KeyChord]) -> Vec<(String, String)> {
        self.bindings(mode)
            .iter()
            .filter(|b| b.keys.len() > prefix.len() && b.keys.starts_with(prefix))
            .map(|b| (display_sequence(&b.keys[prefix.len()..]), b.action.label()))
            .collect()
    }
}

pub(crate) fn fold_leader(
    mode: String,
    keys: Vec<KeyChord>,
    leader: KeyChord,
) -> (String, Vec<KeyChord>) {
    if mode == "normal" && keys.len() > 1 && keys[0] == leader {
        ("leader".into(), keys[1..].to_vec())
    } else {
        (mode, keys)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum KeyOutcome {
    Action(Action),
    /// Waiting for more keys; hints = (key display, action label) of possible continuations.
    Pending {
        hints: Vec<(String, String)>,
    },
    /// Not a binding: send the key to the focused pane.
    PassThrough,
    /// Swallowed (e.g. unbound key in leader/select/resize mode).
    Ignored,
}

/// Per-client key state. Mode changes requested by `enter-mode`, `exit-mode`,
/// `lock` and `select-mode` are applied by [`feed`](Self::feed) itself before the
/// action is returned, so the next key is already read in the right mode even if
/// the caller forgets to call `set_mode`; calling it again is harmless.
#[derive(Debug, Clone)]
pub struct KeyEngine {
    mode: String,
    sticky: bool,
    pending: Vec<KeyChord>,
    /// Start of the current timeout window: a pending chord or a non-sticky mode.
    since: Option<Instant>,
}

impl Default for KeyEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyEngine {
    pub fn new() -> Self {
        Self {
            mode: "normal".into(),
            sticky: false,
            pending: Vec::new(),
            since: None,
        }
    }

    pub fn mode(&self) -> &str {
        &self.mode
    }

    pub fn is_sticky(&self) -> bool {
        self.sticky
    }

    /// Switches mode and clears any pending chord. Non-sticky modes return to
    /// normal after one action, an unbound key, or the timeout.
    pub fn set_mode(&mut self, mode: &str, sticky: bool) {
        self.enter(mode, sticky, Instant::now());
    }

    pub fn exit_mode(&mut self) {
        self.mode = "normal".into();
        self.sticky = false;
        self.pending.clear();
        self.since = None;
    }

    /// True while a chord is half-typed or a one-shot mode (like leader) waits for its key.
    pub fn is_pending(&self) -> bool {
        !self.pending.is_empty() || self.one_shot()
    }

    /// Hints for what can follow right now (for a which-key popup).
    pub fn pending_hints(&self, keymap: &Keymap) -> Vec<(String, String)> {
        keymap.hints(&self.mode, &self.pending)
    }

    fn one_shot(&self) -> bool {
        !self.sticky && self.mode != "normal"
    }

    fn enter(&mut self, mode: &str, sticky: bool, now: Instant) {
        if mode == "normal" {
            self.exit_mode();
            return;
        }
        self.mode = mode.into();
        self.sticky = sticky;
        self.pending.clear();
        self.since = Some(now);
    }

    /// Resolves one key event. In normal mode an unmatched key passes through; when
    /// a multi-chord prefix was pending, only the *last* key passes through (the
    /// buffered prefix keys are dropped), which keeps the engine stateless toward
    /// the pane at the cost of losing a rarely-meant prefix key.
    ///
    /// Pressing the leader key while in leader mode passes it through, so nested
    /// multiplexers and programs that use it still receive it (tmux `send-prefix`).
    pub fn feed(&mut self, keymap: &Keymap, key: &KeyEvent, now: Instant) -> KeyOutcome {
        if key.kind == KeyEventKind::Release {
            return KeyOutcome::Ignored;
        }
        self.tick(keymap, now);
        let chord = KeyChord::from_event(key);

        if self.mode == "normal"
            && self.pending.is_empty()
            && chord == keymap.leader()
            && !keymap.bindings("normal").iter().any(|b| b.keys == [chord])
        {
            self.enter("leader", false, now);
            return KeyOutcome::Pending {
                hints: keymap.hints("leader", &[]),
            };
        }

        let was_pending = !self.pending.is_empty();
        let mut seq = std::mem::take(&mut self.pending);
        seq.push(chord);
        let table = keymap.bindings(&self.mode);

        if let Some(b) = table.iter().find(|b| b.keys == seq) {
            let action = b.action.clone();
            self.after_action(&action, now);
            return KeyOutcome::Action(action);
        }
        if table
            .iter()
            .any(|b| b.keys.len() > seq.len() && b.keys.starts_with(&seq))
        {
            let hints = keymap.hints(&self.mode, &seq);
            self.pending = seq;
            self.since = Some(now);
            return KeyOutcome::Pending { hints };
        }

        match self.mode.as_str() {
            "normal" | "locked" => {
                if self.mode == "normal" {
                    self.since = None;
                }
                KeyOutcome::PassThrough
            }
            "leader" if !was_pending && chord == keymap.leader() => {
                self.exit_mode();
                KeyOutcome::PassThrough
            }
            _ => {
                if self.sticky {
                    self.since = None;
                } else {
                    self.exit_mode();
                }
                KeyOutcome::Ignored
            }
        }
    }

    fn after_action(&mut self, action: &Action, now: Instant) {
        match action {
            Action::EnterMode { mode, sticky } => self.enter(mode, *sticky, now),
            Action::ExitMode => self.exit_mode(),
            Action::Lock => self.enter("locked", true, now),
            Action::SelectMode => self.enter("select", true, now),
            _ if self.one_shot() => self.exit_mode(),
            _ => self.since = None,
        }
    }

    /// Call periodically; returns true if a pending chord or a one-shot mode timed
    /// out and was cancelled.
    pub fn tick(&mut self, keymap: &Keymap, now: Instant) -> bool {
        let (Some(timeout), Some(since)) = (keymap.timeout(), self.since) else {
            return false;
        };
        if !self.is_pending() || now.duration_since(since) < timeout {
            return false;
        }
        self.pending.clear();
        self.since = None;
        if self.one_shot() {
            self.exit_mode();
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Dir;
    use crate::keys::parse_sequence;
    use crossterm::event::{KeyCode, KeyEventState, KeyModifiers};

    fn seq(s: &str) -> Vec<KeyChord> {
        parse_sequence(s).unwrap()
    }

    fn ev(s: &str) -> KeyEvent {
        let c = KeyChord::parse(s).unwrap();
        KeyEvent {
            code: c.code,
            modifiers: c.mods,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn map(user: &[(&str, &str, Option<Action>)]) -> Keymap {
        let leader = KeyChord::parse("ctrl+space").unwrap();
        Keymap::build(
            leader,
            2000,
            crate::presets::bindings("windows", leader, false),
            user.iter()
                .map(|(m, k, a)| (m.to_string(), seq(k), a.clone())),
        )
    }

    fn split_right() -> Action {
        Action::SplitPane {
            direction: Dir::Right,
            size: None,
            command: None,
        }
    }

    #[test]
    fn leader_then_key_fires_and_returns_to_normal() {
        let km = map(&[]);
        let mut e = KeyEngine::new();
        let t = Instant::now();
        let out = e.feed(&km, &ev("ctrl+space"), t);
        let KeyOutcome::Pending { hints } = out else {
            panic!("{out:?}")
        };
        assert!(hints.contains(&("|".into(), "Split pane right".into())));
        assert_eq!(e.mode(), "leader");
        assert!(e.is_pending());
        // Terminals report `|` with Shift; it still matches.
        let pipe = KeyEvent::new(KeyCode::Char('|'), KeyModifiers::SHIFT);
        assert_eq!(e.feed(&km, &pipe, t), KeyOutcome::Action(split_right()));
        assert_eq!(e.mode(), "normal");
        assert!(!e.is_pending());
    }

    #[test]
    fn leader_times_out() {
        let km = map(&[]);
        let mut e = KeyEngine::new();
        let t = Instant::now();
        e.feed(&km, &ev("ctrl+space"), t);
        assert!(!e.tick(&km, t + Duration::from_millis(1999)));
        assert!(e.tick(&km, t + Duration::from_millis(2000)));
        assert_eq!(e.mode(), "normal");
        // A key after the timeout is a plain key again.
        e.feed(&km, &ev("ctrl+space"), t);
        assert_eq!(
            e.feed(&km, &ev("t"), t + Duration::from_secs(3)),
            KeyOutcome::PassThrough
        );
    }

    #[test]
    fn zero_timeout_never_expires() {
        let leader = KeyChord::parse("ctrl+space").unwrap();
        let km = Keymap::build(
            leader,
            0,
            crate::presets::bindings("windows", leader, false),
            [],
        );
        let mut e = KeyEngine::new();
        let t = Instant::now();
        e.feed(&km, &ev("ctrl+space"), t);
        assert!(!e.tick(&km, t + Duration::from_secs(3600)));
        assert_eq!(e.mode(), "leader");
    }

    #[test]
    fn unbound_key_in_leader_is_ignored() {
        let km = map(&[]);
        let mut e = KeyEngine::new();
        let t = Instant::now();
        e.feed(&km, &ev("ctrl+space"), t);
        assert_eq!(e.feed(&km, &ev("f7"), t), KeyOutcome::Ignored);
        assert_eq!(e.mode(), "normal");
    }

    #[test]
    fn leader_twice_sends_leader() {
        let km = map(&[]);
        let mut e = KeyEngine::new();
        let t = Instant::now();
        e.feed(&km, &ev("ctrl+space"), t);
        assert_eq!(e.feed(&km, &ev("ctrl+space"), t), KeyOutcome::PassThrough);
        assert_eq!(e.mode(), "normal");
    }

    #[test]
    fn normal_passthrough_and_direct_keys() {
        let km = map(&[]);
        let mut e = KeyEngine::new();
        let t = Instant::now();
        assert_eq!(e.feed(&km, &ev("a"), t), KeyOutcome::PassThrough);
        assert_eq!(e.feed(&km, &ev("ctrl+c"), t), KeyOutcome::PassThrough);
        assert_eq!(
            e.feed(&km, &ev("alt+3"), t),
            KeyOutcome::Action(Action::GoToTab { index: 3 })
        );
        let mut release = ev("alt+3");
        release.kind = KeyEventKind::Release;
        assert_eq!(e.feed(&km, &release, t), KeyOutcome::Ignored);
    }

    #[test]
    fn sticky_resize_mode_stays_until_exit() {
        let km = map(&[]);
        let mut e = KeyEngine::new();
        let t = Instant::now();
        e.feed(&km, &ev("ctrl+space"), t);
        assert!(matches!(
            e.feed(&km, &ev("r"), t),
            KeyOutcome::Action(Action::EnterMode { .. })
        ));
        assert_eq!(e.mode(), "resize");
        let resize_left = Action::ResizePane {
            direction: Dir::Left,
            amount: 2,
        };
        for _ in 0..3 {
            assert_eq!(
                e.feed(&km, &ev("left"), t),
                KeyOutcome::Action(resize_left.clone())
            );
        }
        assert_eq!(e.feed(&km, &ev("a"), t), KeyOutcome::Ignored);
        assert!(
            !e.tick(&km, t + Duration::from_secs(60)),
            "sticky modes don't time out"
        );
        assert_eq!(e.mode(), "resize");
        assert_eq!(
            e.feed(&km, &ev("esc"), t),
            KeyOutcome::Action(Action::ExitMode)
        );
        assert_eq!(e.mode(), "normal");
    }

    #[test]
    fn locked_passes_everything_but_unlock() {
        let km = map(&[]);
        let mut e = KeyEngine::new();
        let t = Instant::now();
        e.feed(&km, &ev("ctrl+space"), t);
        assert_eq!(e.feed(&km, &ev("l"), t), KeyOutcome::Action(Action::Lock));
        assert_eq!(e.mode(), "locked");
        for k in ["a", "f10", "alt+1", "esc"] {
            assert_eq!(e.feed(&km, &ev(k), t), KeyOutcome::PassThrough, "{k}");
        }
        assert_eq!(
            e.feed(&km, &ev("ctrl+space"), t),
            KeyOutcome::Action(Action::ExitMode)
        );
        assert_eq!(e.mode(), "normal");
    }

    #[test]
    fn select_mode_entered_by_feed() {
        let km = map(&[]);
        let mut e = KeyEngine::new();
        let t = Instant::now();
        e.feed(&km, &ev("ctrl+space"), t);
        assert_eq!(
            e.feed(&km, &ev("["), t),
            KeyOutcome::Action(Action::SelectMode)
        );
        assert_eq!(e.mode(), "select");
        assert_eq!(
            e.feed(&km, &ev("ctrl+c"), t),
            KeyOutcome::Action(Action::CopyAndExit)
        );
        // The server exits select mode after copying.
        e.exit_mode();
        assert_eq!(e.mode(), "normal");
    }

    #[test]
    fn multi_chord_normal_sequence() {
        let km = map(&[("normal", "ctrl+a x", Some(Action::Quit))]);
        let mut e = KeyEngine::new();
        let t = Instant::now();
        let out = e.feed(&km, &ev("ctrl+a"), t);
        assert_eq!(
            out,
            KeyOutcome::Pending {
                hints: vec![("X".into(), "Quit".into())]
            }
        );
        assert_eq!(e.feed(&km, &ev("x"), t), KeyOutcome::Action(Action::Quit));
        // A wrong second key passes through (only that key).
        e.feed(&km, &ev("ctrl+a"), t);
        assert_eq!(e.feed(&km, &ev("y"), t), KeyOutcome::PassThrough);
        assert!(!e.is_pending());
        // And a pending prefix times out.
        e.feed(&km, &ev("ctrl+a"), t);
        assert!(e.tick(&km, t + Duration::from_secs(5)));
        assert!(!e.is_pending());
    }

    #[test]
    fn leader_sequences_in_normal_fold_into_leader_table() {
        let km = map(&[("normal", "ctrl+space q", Some(Action::Quit))]);
        assert!(km.bindings("leader").iter().any(|b| b.keys == seq("q")));
        let mut e = KeyEngine::new();
        let t = Instant::now();
        e.feed(&km, &ev("ctrl+space"), t);
        assert_eq!(e.feed(&km, &ev("q"), t), KeyOutcome::Action(Action::Quit));
    }

    #[test]
    fn user_overrides_and_unbinds() {
        let km = map(&[("leader", "t", Some(Action::Quit)), ("normal", "f10", None)]);
        let t_binding = km
            .bindings("leader")
            .iter()
            .find(|b| b.keys == seq("t"))
            .unwrap();
        assert_eq!(t_binding.action, Action::Quit);
        assert_eq!(t_binding.source, Source::User);
        assert!(!km.bindings("normal").iter().any(|b| b.keys == seq("f10")));
        assert_eq!(
            km.bindings("leader")
                .iter()
                .find(|b| b.keys == seq("p"))
                .unwrap()
                .source,
            Source::Preset
        );
    }

    #[test]
    fn key_hints() {
        let km = map(&[]);
        assert_eq!(km.key_hint(&Action::OpenMenu).as_deref(), Some("F10"));
        assert_eq!(km.key_hint(&split_right()).as_deref(), Some("^Space |"));
        assert_eq!(
            km.key_hint(&Action::GoToTab { index: 2 }).as_deref(),
            Some("Alt+2")
        );
        assert_eq!(km.key_hint(&Action::Quit), None);
        assert_eq!(
            km.key_hint_in("select", &Action::CopyAndExit).as_deref(),
            Some("Enter")
        );
    }

    #[test]
    fn modes_list_builtins_then_user() {
        let km = map(&[("git", "s", Some(Action::Quit))]);
        let modes = km.modes();
        assert_eq!(&modes[..5], MODES);
        assert!(modes.contains(&"git"));
    }

    #[test]
    fn user_mode_non_sticky_returns_after_one_action() {
        let km = map(&[
            (
                "leader",
                "g",
                Some(Action::EnterMode {
                    mode: "git".into(),
                    sticky: false,
                }),
            ),
            ("git", "s", Some(Action::Quit)),
        ]);
        let mut e = KeyEngine::new();
        let t = Instant::now();
        e.feed(&km, &ev("ctrl+space"), t);
        e.feed(&km, &ev("g"), t);
        assert_eq!(e.mode(), "git");
        assert_eq!(e.feed(&km, &ev("s"), t), KeyOutcome::Action(Action::Quit));
        assert_eq!(e.mode(), "normal");
    }
}
