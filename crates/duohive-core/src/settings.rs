//! User settings (TD §7, UI §4.10).

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Colour theme.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    /// Follow the terminal.
    #[default]
    Auto,
    /// Dark palette.
    Dark,
    /// Light palette.
    Light,
}

/// How the org pane is drawn (UI §3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrgLayout {
    /// The connector-line tree (the default for existing installs).
    #[default]
    Tree,
    /// Grouped boxes at 100×30 or more; the compact tree below that.
    Boxes,
}

/// Which desktop alerts are on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AlertSettings {
    /// Alert when an agent needs you.
    pub needs_you: bool,
    /// Alert when an agent finishes.
    pub finished: bool,
}

impl Default for AlertSettings {
    fn default() -> Self {
        Self {
            needs_you: true,
            finished: true,
        }
    }
}

/// Everything in `settings.toml`; missing keys take their defaults.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// What Duo calls the user.
    pub your_name: Option<String>,
    /// Colour theme.
    pub theme: Theme,
    /// Minutes of silence before an agent counts as quiet (minimum 1, D19).
    pub quiet_after_minutes: u32,
    /// Desktop alerts.
    pub alerts: AlertSettings,
    /// Show each agent's last reply where the layout shows it: under its row in the tree, and
    /// on the selected agent's second line in the boxes (UI §4.11).
    pub show_last_reply: bool,
    /// How the org pane is drawn.
    pub org_layout: OrgLayout,
    /// Reduce motion.
    pub reduce_motion: bool,
    /// Key overrides.
    pub keys: BTreeMap<String, String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            your_name: None,
            theme: Theme::Auto,
            quiet_after_minutes: 5,
            alerts: AlertSettings::default(),
            show_last_reply: true,
            org_layout: OrgLayout::Tree,
            reduce_motion: false,
            keys: BTreeMap::new(),
        }
    }
}

impl Settings {
    /// The quiet threshold: `quiet_after_minutes`, at least one minute.
    pub fn quiet_after(&self) -> Duration {
        Duration::from_secs(u64::from(self.quiet_after_minutes.max(1)) * 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_defaults() {
        let s = Settings::default();
        assert_eq!(s.quiet_after_minutes, 5);
        assert_eq!(s.quiet_after(), Duration::from_secs(300));
        assert!(s.show_last_reply);
        assert_eq!(s.theme, Theme::Auto);
        assert!(s.alerts.needs_you && s.alerts.finished);
        assert!(!s.reduce_motion && s.keys.is_empty() && s.your_name.is_none());
    }

    #[test]
    fn quiet_minimum_is_one_minute() {
        let s = Settings {
            quiet_after_minutes: 0,
            ..Default::default()
        };
        assert_eq!(s.quiet_after(), Duration::from_secs(60));
    }

    #[test]
    fn partial_toml_fills_defaults() {
        let s: Settings = toml::from_str("quiet_after_minutes = 9\ntheme = \"dark\"").unwrap();
        assert_eq!(s.quiet_after_minutes, 9);
        assert_eq!(s.theme, Theme::Dark);
        assert!(s.show_last_reply);
    }

    #[test]
    fn a_settings_file_without_org_layout_means_tree() {
        let s: Settings = toml::from_str("theme = \"dark\"").unwrap();
        assert_eq!(s.org_layout, OrgLayout::Tree);
        assert_eq!(Settings::default().org_layout, OrgLayout::Tree);
    }

    #[test]
    fn org_layout_round_trips_in_lowercase() {
        for (layout, word) in [(OrgLayout::Boxes, "boxes"), (OrgLayout::Tree, "tree")] {
            let s = Settings {
                org_layout: layout,
                ..Default::default()
            };
            let text = toml::to_string(&s).unwrap();
            assert!(text.contains(&format!("org_layout = \"{word}\"")), "{text}");
            assert_eq!(
                toml::from_str::<Settings>(&text).unwrap().org_layout,
                layout
            );
        }
    }

    #[test]
    fn an_unknown_org_layout_is_refused_with_its_line() {
        let err = toml::from_str::<Settings>("theme = \"dark\"\norg_layout = \"grid\"")
            .unwrap_err()
            .to_string();
        assert!(err.contains("line 2"), "{err}");
    }

    #[test]
    fn unknown_keys_are_rejected() {
        assert!(toml::from_str::<Settings>("quiet_after_mins = 3").is_err());
        assert!(toml::from_str::<Settings>("[alerts]\nnot_a_key = true").is_err());
    }
}
