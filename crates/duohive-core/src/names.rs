//! Validated names (D4): agent names are identifiers; team and role names are labels.

use std::fmt;

use serde::{Deserialize, Serialize};

const MAX_LEN: usize = 40;
/// The breadcrumb separator, which team and role names may not contain.
const BREADCRUMB_SEP: char = '›';

/// Why a name was refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum NameError {
    /// Nothing, or only whitespace.
    #[error("name is empty")]
    Empty,
    /// Longer than 40 characters.
    #[error("name is longer than 40 characters")]
    TooLong,
    /// Holds characters the rule does not allow.
    #[error("name has characters that are not allowed: {0}")]
    BadChars(String),
}

/// An agent name: `^[A-Za-z0-9][A-Za-z0-9_-]{0,39}$`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct AgentName(String);

impl AgentName {
    /// Checks `s` against the agent-name rule.
    pub fn parse(s: &str) -> Result<Self, NameError> {
        if s.is_empty() {
            return Err(NameError::Empty);
        }
        if s.chars().count() > MAX_LEN {
            return Err(NameError::TooLong);
        }
        let bad_first = s.chars().next().is_some_and(|c| !c.is_ascii_alphanumeric());
        if bad_first {
            return Err(NameError::BadChars(
                "must start with a letter or digit".to_owned(),
            ));
        }
        let bad: String = s
            .chars()
            .filter(|c| !(c.is_ascii_alphanumeric() || *c == '_' || *c == '-'))
            .collect();
        if bad.is_empty() {
            Ok(Self(s.to_owned()))
        } else {
            Err(NameError::BadChars(bad))
        }
    }

    /// The name as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A team or role name: 1-40 characters, trimmed, no control characters, no `›`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct NodeName(String);

impl NodeName {
    /// Trims `s` and checks it against the node-name rule.
    pub fn parse(s: &str) -> Result<Self, NameError> {
        let t = s.trim();
        if t.is_empty() {
            return Err(NameError::Empty);
        }
        if t.chars().count() > MAX_LEN {
            return Err(NameError::TooLong);
        }
        let bad: String = t
            .chars()
            .filter(|c| c.is_control() || *c == BREADCRUMB_SEP)
            .collect();
        if bad.is_empty() {
            Ok(Self(t.to_owned()))
        } else {
            Err(NameError::BadChars(bad))
        }
    }

    /// The name as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

macro_rules! name_glue {
    ($t:ty) => {
        impl fmt::Display for $t {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
        impl TryFrom<String> for $t {
            type Error = NameError;
            fn try_from(s: String) -> Result<Self, NameError> {
                Self::parse(&s)
            }
        }
        impl From<$t> for String {
            fn from(n: $t) -> String {
                n.0
            }
        }
    };
}
name_glue!(AgentName);
name_glue!(NodeName);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_names_follow_d4() {
        for ok in ["auth-service", "squad-1", "Pay_QA", "a"] {
            assert!(AgentName::parse(ok).is_ok(), "{ok}");
        }
        for bad in ["", "-x", "has space", "é", &"a".repeat(41), "a/b"] {
            assert!(AgentName::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn agent_name_length_boundary() {
        assert!(AgentName::parse(&"a".repeat(40)).is_ok());
        assert_eq!(AgentName::parse(&"a".repeat(41)), Err(NameError::TooLong));
        assert_eq!(AgentName::parse(""), Err(NameError::Empty));
    }

    #[test]
    fn node_names_reject_breadcrumb_separator() {
        assert!(NodeName::parse("A › B").is_err());
        assert!(NodeName::parse("  ").is_err());
    }

    #[test]
    fn node_names_are_trimmed_and_reject_control_chars() {
        assert_eq!(NodeName::parse("  Payments ").unwrap().as_str(), "Payments");
        assert!(NodeName::parse("a\nb").is_err());
        assert!(NodeName::parse(&"é".repeat(40)).is_ok());
        assert!(NodeName::parse(&"é".repeat(41)).is_err());
    }

    #[derive(Deserialize, Serialize, Debug)]
    struct Wrap {
        n: AgentName,
    }

    #[test]
    fn serde_goes_through_parse() {
        let ok: Wrap = toml::from_str("n = \"auth-service\"").unwrap();
        assert_eq!(ok.n.as_str(), "auth-service");
        assert_eq!(toml::to_string(&ok).unwrap().trim(), "n = \"auth-service\"");
        assert!(toml::from_str::<Wrap>("n = \"bad name\"").is_err());
    }
}
