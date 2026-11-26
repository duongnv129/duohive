//! A team's colour (TD §7, UI §2.3): one of four named hues, never a free value, so every
//! hue's contrast is known. The core only names them; the theme says how they render.

use std::fmt;

use serde::{Deserialize, Serialize};

/// One of the four team colours, in palette order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TeamColour {
    /// Teal.
    Teal,
    /// Violet.
    Violet,
    /// Slate.
    Slate,
    /// Orchid.
    Orchid,
}

/// A colour name that is not one of the four.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColourError {
    /// What was written.
    pub given: String,
}

impl fmt::Display for ColourError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown colour \"{}\" · teal, violet, slate or orchid",
            self.given
        )
    }
}

impl std::error::Error for ColourError {}

impl TeamColour {
    /// The palette, in the order colours are handed out.
    pub const ALL: [TeamColour; 4] = [Self::Teal, Self::Violet, Self::Slate, Self::Orchid];

    /// The lowercase name, as written in `org.toml`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Teal => "teal",
            Self::Violet => "violet",
            Self::Slate => "slate",
            Self::Orchid => "orchid",
        }
    }

    /// The colour a name stands for.
    pub fn parse(s: &str) -> Result<TeamColour, ColourError> {
        Self::ALL
            .into_iter()
            .find(|c| c.name() == s)
            .ok_or_else(|| ColourError {
                given: s.to_owned(),
            })
    }

    /// The first palette colour `used` doesn't hold; with all four taken, `ALL[len % 4]`
    /// (the fifth sibling takes teal, the sixth violet).
    pub fn next_free(used: &[TeamColour]) -> TeamColour {
        Self::ALL
            .into_iter()
            .find(|c| !used.contains(c))
            .unwrap_or(Self::ALL[used.len() % Self::ALL.len()])
    }

    /// The colour at `position` among the siblings, as a colourless node derives it.
    pub fn at(position: usize) -> TeamColour {
        Self::ALL[position % Self::ALL.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::TeamColour::{Orchid, Slate, Teal, Violet};
    use super::*;
    use crate::org::{NodeKey, RoleKey};
    use crate::testkit::{example_org, n};

    #[test]
    fn parse_accepts_the_four_names_and_refuses_others() {
        for c in TeamColour::ALL {
            assert_eq!(TeamColour::parse(c.name()), Ok(c));
        }
        let err = TeamColour::parse("tael").unwrap_err();
        assert_eq!(
            err.to_string(),
            "unknown colour \"tael\" · teal, violet, slate or orchid"
        );
        assert!(TeamColour::parse("Teal").is_err());
    }

    #[test]
    fn serde_uses_lowercase_names() {
        assert_eq!(serde_json::to_string(&Teal).unwrap(), "\"teal\"");
        assert_eq!(
            serde_json::from_str::<TeamColour>("\"orchid\"").unwrap(),
            Orchid
        );
        assert!(serde_json::from_str::<TeamColour>("\"Teal\"").is_err());
    }

    #[test]
    fn next_free_takes_the_first_unused_then_wraps() {
        assert_eq!(TeamColour::next_free(&[]), Teal);
        assert_eq!(TeamColour::next_free(&[Teal, Slate]), Violet);
        assert_eq!(TeamColour::next_free(&[Teal, Violet, Slate, Orchid]), Teal);
        assert_eq!(
            TeamColour::next_free(&[Teal, Violet, Slate, Orchid, Teal]),
            Violet
        );
    }

    #[test]
    fn colour_of_derives_from_position_when_unset() {
        let mut org = example_org();
        let mut platform = org.teams[0].clone();
        platform.name = crate::names::NodeName::parse("Platform").unwrap();
        org.teams.push(platform);
        for t in &mut org.teams {
            t.color = None;
        }
        for r in &mut org.roles {
            r.color = None;
        }
        let teams: Vec<_> = org.teams.iter().map(|t| t.name.clone()).collect();
        let root = org.roles[0].name.clone();
        assert_eq!(org.colour_of(&NodeKey::Team(teams[0].clone())), Some(Teal));
        assert_eq!(
            org.colour_of(&NodeKey::Team(teams[1].clone())),
            Some(Violet)
        );
        let at_root = NodeKey::Role(RoleKey {
            team: None,
            role: root,
        });
        assert_eq!(org.colour_of(&at_root), Some(Slate));
        let nested = NodeKey::Role(RoleKey {
            team: Some(teams[0].clone()),
            role: org.teams[0].roles[0].name.clone(),
        });
        assert_eq!(org.colour_of(&nested), Some(Teal));
        assert_eq!(
            org.colour_of(&NodeKey::Agent(n("auth-service"))),
            Some(Teal)
        );
        assert_eq!(org.colour_of(&NodeKey::Duo), None);
    }
}
