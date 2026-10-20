//! The key bindings: defaults, settings overrides and the conflict rules (UI §4.10,
//! §4.11, TD §7).

use std::collections::BTreeMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// The actions a key can be bound to (TD §7's `[keys]` ids).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ActionId {
    /// Give work to the selected agent or role.
    GiveWork,
    /// Start or stop the selected agent.
    StartStop,
    /// New here….
    New,
    /// Edit the selection.
    Edit,
    /// Move to….
    Move,
    /// Delete the selection.
    Delete,
    /// Zoom the selected agent.
    Zoom,
    /// Last reply on or off.
    LastReply,
    /// Open `org.toml` in `$EDITOR`.
    OpenOrg,
    /// Open Settings.
    Settings,
    /// Open the worktree list.
    Worktrees,
    /// Filter the tree by state.
    Filter,
    /// Open this key map.
    KeyMap,
    /// Approve a card.
    CardApprove,
    /// Edit a card.
    CardEdit,
    /// Reject a card.
    CardReject,
}

impl ActionId {
    /// Every action, in the order the key map and the editor list them.
    pub const ALL: [Self; 16] = [
        Self::GiveWork,
        Self::StartStop,
        Self::New,
        Self::Edit,
        Self::Move,
        Self::Delete,
        Self::Zoom,
        Self::LastReply,
        Self::OpenOrg,
        Self::Settings,
        Self::Worktrees,
        Self::Filter,
        Self::KeyMap,
        Self::CardApprove,
        Self::CardEdit,
        Self::CardReject,
    ];

    /// The action's `settings.toml` id.
    pub fn id(self) -> &'static str {
        match self {
            Self::GiveWork => "give_work",
            Self::StartStop => "start_stop",
            Self::New => "new",
            Self::Edit => "edit",
            Self::Move => "move",
            Self::Delete => "delete",
            Self::Zoom => "zoom",
            Self::LastReply => "last_reply",
            Self::OpenOrg => "open_org",
            Self::Settings => "settings",
            Self::Worktrees => "worktrees",
            Self::Filter => "filter",
            Self::KeyMap => "key_map",
            Self::CardApprove => "card_approve",
            Self::CardEdit => "card_edit",
            Self::CardReject => "card_reject",
        }
    }

    /// The action an id names, if it names one.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.id() == id)
    }

    /// The action's name as the key map and the editor show it.
    pub fn label(self) -> &'static str {
        match self {
            Self::GiveWork => "Give work",
            Self::StartStop => "Start / stop",
            Self::New => "New",
            Self::Edit => "Edit",
            Self::Move => "Move",
            Self::Delete => "Delete",
            Self::Zoom => "Zoom",
            Self::LastReply => "Last reply",
            Self::OpenOrg => "Open org.toml",
            Self::Settings => "Settings",
            Self::Worktrees => "Worktrees",
            Self::Filter => "Filter",
            Self::KeyMap => "Key map",
            Self::CardApprove => "Approve card",
            Self::CardEdit => "Edit card",
            Self::CardReject => "Reject card",
        }
    }
}

/// Where a binding applies (UI §4.10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Context {
    /// Every pane.
    Everywhere,
    /// The org tree.
    OrgPane,
    /// The Needs you list.
    NeedsYou,
    /// The agent pane in Selected mode.
    AgentPane,
    /// The chat input and its cards.
    Chat,
}

/// One key and its modifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding {
    /// The key.
    pub code: KeyCode,
    /// The modifiers held with it.
    pub mods: KeyModifiers,
}

impl Binding {
    /// How `settings.toml` and the screens write this binding.
    pub fn label(&self) -> String {
        let base = match self.code {
            KeyCode::Char(' ') => "space".to_owned(),
            KeyCode::Char(c) => {
                let c = if self.mods.contains(KeyModifiers::SHIFT) {
                    c.to_ascii_lowercase()
                } else {
                    c
                };
                c.to_string()
            }
            KeyCode::BackTab => return "shift+tab".to_owned(),
            KeyCode::Tab => "tab".to_owned(),
            KeyCode::Esc => "esc".to_owned(),
            KeyCode::Enter => "enter".to_owned(),
            KeyCode::Up => "up".to_owned(),
            KeyCode::Down => "down".to_owned(),
            KeyCode::Left => "left".to_owned(),
            KeyCode::Right => "right".to_owned(),
            KeyCode::Home => "home".to_owned(),
            KeyCode::End => "end".to_owned(),
            KeyCode::PageUp => "pgup".to_owned(),
            KeyCode::PageDown => "pgdn".to_owned(),
            KeyCode::Backspace => "backspace".to_owned(),
            KeyCode::Delete => "delete".to_owned(),
            KeyCode::Insert => "insert".to_owned(),
            KeyCode::F(n) => format!("f{n}"),
            _ => return String::new(),
        };
        let mut text = String::new();
        for (held, name) in [
            (KeyModifiers::CONTROL, "ctrl"),
            (KeyModifiers::ALT, "alt"),
            (KeyModifiers::SHIFT, "shift"),
        ] {
            if self.mods.contains(held)
                && !(held == KeyModifiers::SHIFT && self.code == KeyCode::BackTab)
            {
                text.push_str(name);
                text.push('+');
            }
        }
        text.push_str(&base);
        text
    }

    /// Whether this key is reserved: no action may take it (UI §4.10).
    pub fn is_reserved(&self) -> bool {
        let ctrl = self.mods.contains(KeyModifiers::CONTROL);
        match self.code {
            KeyCode::Tab | KeyCode::BackTab | KeyCode::Esc | KeyCode::Enter => true,
            KeyCode::PageUp | KeyCode::PageDown => true,
            KeyCode::Up | KeyCode::Down => self.mods.is_empty() || ctrl,
            KeyCode::Left | KeyCode::Right => self.mods.is_empty(),
            KeyCode::Char('q') => !self.mods.contains(KeyModifiers::ALT),
            KeyCode::Char('c') => ctrl,
            KeyCode::Char('@' | '/') => true,
            _ => false,
        }
    }

    /// Whether `k` presses this binding.
    fn matches(&self, k: &KeyEvent) -> bool {
        let ctrl = self.mods.contains(KeyModifiers::CONTROL);
        let alt = self.mods.contains(KeyModifiers::ALT);
        if ctrl != k.modifiers.contains(KeyModifiers::CONTROL)
            || alt != k.modifiers.contains(KeyModifiers::ALT)
        {
            return false;
        }
        match (self.code, k.code) {
            (KeyCode::Char(a), KeyCode::Char(b))
                if a.is_ascii_alphabetic() && b.is_ascii_alphabetic() =>
            {
                if self.mods.contains(KeyModifiers::SHIFT) {
                    a.eq_ignore_ascii_case(&b)
                        && (k.modifiers.contains(KeyModifiers::SHIFT) || b.is_ascii_uppercase())
                } else {
                    a == b && !k.modifiers.contains(KeyModifiers::SHIFT)
                }
            }
            (a, b) => a == b,
        }
    }
}

/// Reads `"g"`, `"shift+n"`, `"ctrl+g"`, `"f2"`, `"space"`; `None` when it doesn't parse.
pub fn parse_binding(text: &str) -> Option<Binding> {
    let parts: Vec<&str> = text.split('+').collect();
    let (key, mods_text) = parts.split_last()?;
    let mut mods = KeyModifiers::empty();
    for part in mods_text {
        match part.trim().to_ascii_lowercase().as_str() {
            "ctrl" => mods.insert(KeyModifiers::CONTROL),
            "alt" => mods.insert(KeyModifiers::ALT),
            "shift" => mods.insert(KeyModifiers::SHIFT),
            _ => return None,
        }
    }
    let mut code = parse_code(key)?;
    if mods.contains(KeyModifiers::SHIFT) {
        match code {
            KeyCode::Tab => {
                code = KeyCode::BackTab;
                mods.remove(KeyModifiers::SHIFT);
            }
            KeyCode::Char(c) => code = KeyCode::Char(c.to_ascii_uppercase()),
            _ => {}
        }
    }
    Some(Binding { code, mods })
}

fn parse_code(key: &str) -> Option<KeyCode> {
    let lower = key.to_ascii_lowercase();
    let code = match lower.as_str() {
        "space" => KeyCode::Char(' '),
        "tab" => KeyCode::Tab,
        "backtab" => KeyCode::BackTab,
        "esc" | "escape" => KeyCode::Esc,
        "enter" | "return" => KeyCode::Enter,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pgup" | "pageup" => KeyCode::PageUp,
        "pgdn" | "pagedown" => KeyCode::PageDown,
        "backspace" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "insert" => KeyCode::Insert,
        f if f.len() > 1 && f.starts_with('f') => KeyCode::F(f[1..].parse().ok()?),
        _ => {
            let mut chars = key.chars();
            let c = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            KeyCode::Char(c)
        }
    };
    Some(code)
}

/// What a binding may not collide with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RebindError {
    /// The key is reserved (UI §4.10).
    Reserved(String),
    /// Another action in a shared context already has this key.
    Conflict {
        /// The key, as written.
        key: String,
        /// The action that holds it.
        action: ActionId,
    },
}

/// Every action's binding, by action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keymap {
    bindings: BTreeMap<ActionId, Binding>,
}

impl Default for Keymap {
    fn default() -> Self {
        Self {
            bindings: defaults(),
        }
    }
}

/// The spec's defaults (UI §4.11).
fn defaults() -> BTreeMap<ActionId, Binding> {
    let mut out = BTreeMap::new();
    for (action, text) in [
        (ActionId::GiveWork, " "),
        (ActionId::StartStop, "s"),
        (ActionId::New, "n"),
        (ActionId::Edit, "e"),
        (ActionId::Move, "m"),
        (ActionId::Delete, "d"),
        (ActionId::Zoom, "z"),
        (ActionId::LastReply, "v"),
        (ActionId::OpenOrg, "o"),
        (ActionId::Settings, ","),
        (ActionId::Worktrees, "w"),
        (ActionId::Filter, "f"),
        (ActionId::KeyMap, "?"),
        (ActionId::CardApprove, "y"),
        (ActionId::CardEdit, "e"),
        (ActionId::CardReject, "n"),
    ] {
        if let Some(b) = parse_binding(text) {
            out.insert(action, b);
        }
    }
    out
}

impl Keymap {
    /// The default bindings with `saved`'s overrides applied; a notice for each entry
    /// that is ignored (UI §4.10).
    pub fn from_settings(saved: &BTreeMap<String, String>) -> (Self, Vec<String>) {
        let mut out = Self::default();
        let mut notices = Vec::new();
        for (id, text) in saved {
            let Some(action) = ActionId::from_id(id) else {
                notices.push(format!(
                    "ignored in settings.toml: {id} · using the default"
                ));
                continue;
            };
            let Some(binding) = parse_binding(text) else {
                notices.push(format!(
                    "ignored in settings.toml: {id} · using the default"
                ));
                continue;
            };
            out.bindings.insert(action, binding);
        }
        (out, notices)
    }

    /// The action's binding.
    pub fn key(&self, action: ActionId) -> Binding {
        self.bindings.get(&action).copied().unwrap_or(Binding {
            code: KeyCode::Null,
            mods: KeyModifiers::empty(),
        })
    }

    /// The action `k` presses in `context`, if any.
    pub fn action_for(&self, context: Context, k: &KeyEvent) -> Option<ActionId> {
        ActionId::ALL.into_iter().find(|a| {
            self.key(*a).matches(k)
                && contexts(*a)
                    .iter()
                    .any(|c| *c == context || *c == Context::Everywhere)
        })
    }

    /// The card action `k` presses while a card has focus; other keys type in the chat.
    pub fn card_action(&self, k: &KeyEvent) -> Option<ActionId> {
        [
            ActionId::CardApprove,
            ActionId::CardEdit,
            ActionId::CardReject,
        ]
        .into_iter()
        .find(|a| self.key(*a).matches(k))
    }

    /// Binds `action` to `binding` unless it is reserved or already taken.
    pub fn rebind(&mut self, action: ActionId, binding: Binding) -> Result<(), RebindError> {
        if binding.is_reserved() {
            return Err(RebindError::Reserved(binding.label()));
        }
        if let Some(other) = ActionId::ALL.into_iter().find(|other| {
            *other != action && self.key(*other) == binding && share_contexts(action, *other)
        }) {
            return Err(RebindError::Conflict {
                key: binding.label(),
                action: other,
            });
        }
        self.bindings.insert(action, binding);
        Ok(())
    }

    /// Puts the action back on its default key.
    pub fn reset(&mut self, action: ActionId) {
        if let Some(default) = defaults().get(&action) {
            self.bindings.insert(action, *default);
        }
    }

    /// Only the changed bindings, as `settings.toml` holds them.
    pub fn overrides(&self) -> BTreeMap<String, String> {
        let default = defaults();
        self.bindings
            .iter()
            .filter(|(action, binding)| default.get(action) != Some(binding))
            .map(|(action, binding)| (action.id().to_owned(), binding.label()))
            .collect()
    }
}

/// The contexts an action's key works in (UI §4.10).
pub fn contexts(action: ActionId) -> &'static [Context] {
    match action {
        ActionId::GiveWork | ActionId::Zoom => &[Context::OrgPane, Context::AgentPane],
        ActionId::StartStop
        | ActionId::New
        | ActionId::Edit
        | ActionId::Move
        | ActionId::Delete
        | ActionId::LastReply
        | ActionId::OpenOrg => &[Context::OrgPane],
        ActionId::Settings | ActionId::Worktrees | ActionId::Filter | ActionId::KeyMap => {
            &[Context::Everywhere]
        }
        ActionId::CardApprove | ActionId::CardEdit | ActionId::CardReject => &[Context::Chat],
    }
}

/// Whether two actions can clash: same context, or either is everywhere.
fn share_contexts(a: ActionId, b: ActionId) -> bool {
    contexts(a).iter().any(|x| {
        contexts(b)
            .iter()
            .any(|y| x == y || *x == Context::Everywhere || *y == Context::Everywhere)
    })
}

/// The key map's scroll (UI §4.11); the wide layout shows everything.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyMapState {
    /// The first row shown in the one-column layout.
    pub scroll: usize,
}

/// The key bindings editor's state (UI §4.10).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeysState {
    /// The selected row, an index into [`ActionId::ALL`].
    pub cursor: usize,
    /// Waiting for the next key.
    pub capturing: bool,
    /// The refusal on the row.
    pub notice: Option<String>,
    /// The first row shown when the list doesn't fit.
    pub scroll: usize,
}

impl KeysState {
    /// A new editor on the first action.
    pub fn new() -> Self {
        Self::default()
    }

    /// `↑` or `↓` within the actions.
    pub fn step(&mut self, down: bool) {
        let last = ActionId::ALL.len().saturating_sub(1);
        self.cursor = if down {
            (self.cursor + 1).min(last)
        } else {
            self.cursor.saturating_sub(1)
        };
        self.notice = None;
    }

    /// The action under the cursor.
    pub fn action(&self) -> ActionId {
        ActionId::ALL[self.cursor.min(ActionId::ALL.len() - 1)]
    }
}

/// The binding a captured key press makes, unless it is a modifier alone.
pub fn captured_binding(k: &KeyEvent) -> Option<Binding> {
    if matches!(k.code, KeyCode::Null | KeyCode::Modifier(_)) {
        return None;
    }
    Some(Binding {
        code: k.code,
        mods: k.modifiers,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn bind(s: &str) -> Binding {
        parse_binding(s).unwrap_or_else(|| panic!("{s} should parse"))
    }

    #[test]
    fn defaults_match_the_spec() {
        let k = Keymap::default();
        let want = [
            (ActionId::GiveWork, "space"),
            (ActionId::StartStop, "s"),
            (ActionId::New, "n"),
            (ActionId::Edit, "e"),
            (ActionId::Move, "m"),
            (ActionId::Delete, "d"),
            (ActionId::Zoom, "z"),
            (ActionId::LastReply, "v"),
            (ActionId::OpenOrg, "o"),
            (ActionId::Settings, ","),
            (ActionId::Worktrees, "w"),
            (ActionId::Filter, "f"),
            (ActionId::KeyMap, "?"),
            (ActionId::CardApprove, "y"),
            (ActionId::CardEdit, "e"),
            (ActionId::CardReject, "n"),
        ];
        for (action, label) in want {
            assert_eq!(k.key(action).label(), label, "{action:?}");
        }
    }

    #[test]
    fn every_action_has_a_default_key() {
        for action in ActionId::ALL {
            assert!(
                !Keymap::default().key(action).label().is_empty(),
                "{action:?}"
            );
        }
    }

    #[test]
    fn parse_and_label_round_trip() {
        for text in ["ctrl+g", "shift+n", "f2", " ", "enter", "ctrl+up"] {
            let binding = parse_binding(text).unwrap_or_else(|| panic!("{text}"));
            let label = binding.label();
            assert_eq!(parse_binding(&label), Some(binding), "{text} -> {label}");
        }
        assert_eq!(bind(" ").label(), "space");
        assert_eq!(bind("shift+n").label(), "shift+n");
    }

    #[test]
    fn rebind_applies_everywhere() {
        let mut k = Keymap::default();
        assert_eq!(k.rebind(ActionId::Zoom, bind("x")), Ok(()));
        assert_eq!(
            k.action_for(Context::OrgPane, &key('x')),
            Some(ActionId::Zoom)
        );
        assert_eq!(
            k.action_for(Context::AgentPane, &key('x')),
            Some(ActionId::Zoom)
        );
        assert_eq!(k.action_for(Context::OrgPane, &key('z')), None);
        assert_eq!(k.action_for(Context::AgentPane, &key('z')), None);
    }

    #[test]
    fn two_context_action_rebinds_both() {
        let mut k = Keymap::default();
        assert_eq!(k.rebind(ActionId::GiveWork, bind("x")), Ok(()));
        assert_eq!(
            k.action_for(Context::OrgPane, &key('x')),
            Some(ActionId::GiveWork)
        );
        assert_eq!(
            k.action_for(Context::AgentPane, &key('x')),
            Some(ActionId::GiveWork)
        );
        assert_eq!(k.action_for(Context::OrgPane, &key(' ')), None);
    }

    #[test]
    fn conflict_refused_in_same_context() {
        let mut k = Keymap::default();
        assert_eq!(
            k.rebind(ActionId::Edit, bind("s")),
            Err(RebindError::Conflict {
                key: "s".to_owned(),
                action: ActionId::StartStop,
            })
        );
        assert_eq!(k.key(ActionId::Edit), bind("e"));
    }

    #[test]
    fn same_key_allowed_in_different_contexts() {
        let k = Keymap::default();
        assert_eq!(
            k.action_for(Context::OrgPane, &key('e')),
            Some(ActionId::Edit)
        );
        assert_eq!(
            k.action_for(Context::Chat, &key('e')),
            Some(ActionId::CardEdit)
        );
    }

    #[test]
    fn everywhere_key_conflicts_with_pane_key() {
        let mut k = Keymap::default();
        assert_eq!(
            k.rebind(ActionId::Filter, bind("s")),
            Err(RebindError::Conflict {
                key: "s".to_owned(),
                action: ActionId::StartStop,
            })
        );
    }

    #[test]
    fn reserved_keys_refused() {
        for text in ["q", "ctrl+up", "enter", "esc"] {
            let mut k = Keymap::default();
            let binding = bind(text);
            assert_eq!(
                k.rebind(ActionId::New, binding),
                Err(RebindError::Reserved(binding.label())),
                "{text}"
            );
        }
    }

    #[test]
    fn conflict_check_covers_reserved_keys() {
        let mut k = Keymap::default();
        assert_eq!(
            k.rebind(ActionId::New, bind("tab")),
            Err(RebindError::Reserved("tab".to_owned()))
        );
    }

    #[test]
    fn overrides_only_changed_bindings() {
        let mut k = Keymap::default();
        assert!(k.overrides().is_empty());
        assert_eq!(k.rebind(ActionId::New, bind("x")), Ok(()));
        let got: BTreeMap<String, String> =
            [("new".to_owned(), "x".to_owned())].into_iter().collect();
        assert_eq!(k.overrides(), got);
    }

    #[test]
    fn backspace_restores_default() {
        let mut k = Keymap::default();
        assert_eq!(k.rebind(ActionId::New, bind("x")), Ok(()));
        k.reset(ActionId::New);
        assert_eq!(k.key(ActionId::New), bind("n"));
        assert!(k.overrides().is_empty());
    }

    #[test]
    fn invalid_saved_key_falls_back_to_default() {
        let saved: BTreeMap<String, String> =
            [("new".to_owned(), "???".to_owned())].into_iter().collect();
        let (k, notices) = Keymap::from_settings(&saved);
        assert_eq!(notices.len(), 1);
        assert_eq!(k.key(ActionId::New), bind("n"));
    }

    #[test]
    fn unknown_id_notice_shown() {
        let saved: BTreeMap<String, String> = [
            ("nope".to_owned(), "x".to_owned()),
            ("zoom".to_owned(), "???".to_owned()),
        ]
        .into_iter()
        .collect();
        let (k, notices) = Keymap::from_settings(&saved);
        assert_eq!(notices.len(), 2);
        assert_eq!(k.key(ActionId::Zoom), bind("z"));
    }
}
