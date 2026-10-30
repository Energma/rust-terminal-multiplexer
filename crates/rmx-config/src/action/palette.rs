//! The default set of actions offered by the command palette.

use super::{Action, Dir, LAYOUTS};

/// Every action users can pick from the command palette, with sensible default
/// arguments. Select-mode motions, scrolling, `exit-mode` and actions that need
/// free-form arguments (`run`, `send-keys`) are left out.
pub fn palette_actions() -> Vec<Action> {
    use Action::*;
    let mut v = vec![NewTab, CloseTab, RenameTab, NextTab, PrevTab];
    v.extend((1..=9).map(|index| GoToTab { index }));
    v.extend([MoveTab { delta: -1 }, MoveTab { delta: 1 }]);
    for direction in [Dir::Right, Dir::Down] {
        v.push(SplitPane {
            direction,
            size: None,
            command: None,
        });
    }
    v.push(ClosePane);
    v.extend(Dir::ALL.map(|direction| MoveFocus { direction }));
    v.extend(Dir::ALL.map(|direction| ResizePane {
        direction,
        amount: 2,
    }));
    v.push(TogglePaneZoom);
    v.extend(Dir::ALL.map(|direction| SwapPane { direction }));
    v.push(ApplyLayout {
        name: "next".into(),
    });
    v.extend(LAYOUTS.iter().map(|n| ApplyLayout { name: (*n).into() }));
    v.extend([
        ToggleSyncInput,
        SelectMode,
        Find,
        Copy,
        Paste,
        OpenMenu,
        ShowHelp,
        WorkspaceSwitcher,
        NewWorkspace,
        RenameWorkspace,
        NextWorkspace,
        PrevWorkspace,
        Detach,
        Quit,
        ReloadConfig,
        OpenConfig,
        OpenEditor,
        EnterMode {
            mode: "resize".into(),
            sticky: true,
        },
        Lock,
    ]);
    v
}
