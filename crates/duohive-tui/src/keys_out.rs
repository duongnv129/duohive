//! Keys typed in Typing mode, as the agent's pane should receive them (UI §4.1.2).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use duohive_core::view::{PaneInput, PaneKey};

/// The highest function key a pane takes.
const MAX_F_KEY: u8 = 12;

/// What `k` sends to the agent's pane; `None` for a key the pane has no name for.
///
/// `ctrl+<char>` and `alt+<char>` become named keys; other characters are sent as text,
/// with shift already applied by the terminal. Leaving Typing mode (`ctrl+q`) is decided
/// before this is called.
pub fn key_to_input(k: KeyEvent) -> Option<PaneInput> {
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    let alt = k.modifiers.contains(KeyModifiers::ALT);
    let key = match k.code {
        KeyCode::Char(c) if ctrl => PaneKey::Ctrl(c.to_ascii_lowercase()),
        KeyCode::Char(c) if alt => PaneKey::Alt(c),
        KeyCode::Char(c) => return Some(PaneInput::Text(c.to_string())),
        KeyCode::Enter => PaneKey::Enter,
        KeyCode::Tab => PaneKey::Tab,
        KeyCode::BackTab => PaneKey::BackTab,
        KeyCode::Backspace => PaneKey::Backspace,
        KeyCode::Delete => PaneKey::Delete,
        KeyCode::Esc => PaneKey::Escape,
        KeyCode::Up => PaneKey::Up,
        KeyCode::Down => PaneKey::Down,
        KeyCode::Left => PaneKey::Left,
        KeyCode::Right => PaneKey::Right,
        KeyCode::Home => PaneKey::Home,
        KeyCode::End => PaneKey::End,
        KeyCode::PageUp => PaneKey::PageUp,
        KeyCode::PageDown => PaneKey::PageDown,
        KeyCode::Insert => PaneKey::Insert,
        KeyCode::F(n) if (1..=MAX_F_KEY).contains(&n) => PaneKey::F(n),
        _ => return None,
    };
    Some(PaneInput::Key(key))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    #[test]
    fn keys_map_to_pane_input() {
        let none = KeyModifiers::NONE;
        let cases = [
            (
                k(KeyCode::Char('a'), none),
                Some(PaneInput::Text("a".into())),
            ),
            (
                k(KeyCode::Char('A'), KeyModifiers::SHIFT),
                Some(PaneInput::Text("A".into())),
            ),
            (
                k(KeyCode::Char('é'), none),
                Some(PaneInput::Text("é".into())),
            ),
            (
                k(
                    KeyCode::Char('C'),
                    KeyModifiers::CONTROL | KeyModifiers::SHIFT,
                ),
                Some(PaneInput::Key(PaneKey::Ctrl('c'))),
            ),
            (
                k(KeyCode::Char('x'), KeyModifiers::ALT),
                Some(PaneInput::Key(PaneKey::Alt('x'))),
            ),
            (
                k(KeyCode::BackTab, KeyModifiers::SHIFT),
                Some(PaneInput::Key(PaneKey::BackTab)),
            ),
            (
                k(KeyCode::Backspace, none),
                Some(PaneInput::Key(PaneKey::Backspace)),
            ),
            (
                k(KeyCode::Delete, none),
                Some(PaneInput::Key(PaneKey::Delete)),
            ),
            (k(KeyCode::Down, none), Some(PaneInput::Key(PaneKey::Down))),
            (k(KeyCode::Left, none), Some(PaneInput::Key(PaneKey::Left))),
            (
                k(KeyCode::Right, none),
                Some(PaneInput::Key(PaneKey::Right)),
            ),
            (k(KeyCode::Home, none), Some(PaneInput::Key(PaneKey::Home))),
            (k(KeyCode::End, none), Some(PaneInput::Key(PaneKey::End))),
            (
                k(KeyCode::PageDown, none),
                Some(PaneInput::Key(PaneKey::PageDown)),
            ),
            (
                k(KeyCode::Insert, none),
                Some(PaneInput::Key(PaneKey::Insert)),
            ),
            (
                k(KeyCode::F(12), none),
                Some(PaneInput::Key(PaneKey::F(12))),
            ),
            (k(KeyCode::F(13), none), None),
            (k(KeyCode::CapsLock, none), None),
        ];
        for (key, want) in cases {
            assert_eq!(key_to_input(key), want, "{key:?}");
        }
    }
}
