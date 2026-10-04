//! Validation results for org edits.

/// A problem with one field, addressed by its path in the org.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldError {
    /// Field path, e.g. `team[Payments].role[Backend].agent[billing-api].flags`.
    pub path: String,
    /// What is wrong, in a sentence.
    pub message: String,
}
