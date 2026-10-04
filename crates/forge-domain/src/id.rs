use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

macro_rules! define_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub struct $name(Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            pub fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }

            /// Returns the underlying UUID, for persistence layers that
            /// bind native `uuid` columns.
            pub fn into_uuid(self) -> Uuid {
                self.0
            }

            pub fn as_uuid(&self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

define_id!(TenantId);
define_id!(JobId);
define_id!(JobVersionId);
define_id!(ExecutionId);
define_id!(WorkerId);
define_id!(QueueId);
define_id!(WorkflowId);

// An application: the PowerJob-style grouping that jobs belong to.
// See `application::Application` for the type and why it exists.
define_id!(ApplicationId);

// A deployment environment such as dev, staging or prod.
// See `application::Environment`.
define_id!(EnvironmentId);

// An editable revision of a job's desired state. Distinct from `JobVersionId`:
// a published version is immutable and is what executions point back to, while
// a revision is the draft an operator edits and a migration copies.
define_id!(JobRevisionId);

// A record of one cross-environment migration attempt.
define_id!(MigrationId);

/// A validated, human-typeable identifier for a tenant, application, or
/// environment.
///
/// Distinct from the generated ids above because a slug is what an operator
/// types and what appears in a URL. That makes its validation rules stricter
/// than a UUID's: it must survive a path segment, a DNS label, and being read
/// aloud in an incident callout.
///
/// Invariants enforced here rather than at the edge, so a slug arriving from any
/// source (API, import, migration manifest) obeys the same rules:
///
/// - lowercase ASCII letters, digits, and single hyphens;
/// - 2-32 characters;
/// - no leading or trailing hyphen;
/// - no consecutive hyphens.
///
/// The last rule is not cosmetic: `payments--prod` and `payments-prod` are
/// distinct strings that a human reads identically, and a mistyped slug silently
/// addressing the wrong environment is the failure mode this prevents.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Slug(String);

impl Slug {
    pub const MAX_LEN: usize = 32;
    pub const MIN_LEN: usize = 2;

    /// Returns why a candidate slug is unacceptable, or `Ok` when it is valid.
    ///
    /// Returns the reason rather than a bool so an API can tell the caller what
    /// to fix, and so the console and CLI report the same message.
    pub fn validate(candidate: &str) -> Result<(), SlugError> {
        if candidate.len() < Self::MIN_LEN {
            return Err(SlugError::TooShort {
                len: candidate.len(),
                min: Self::MIN_LEN,
            });
        }
        if candidate.len() > Self::MAX_LEN {
            return Err(SlugError::TooLong {
                len: candidate.len(),
                max: Self::MAX_LEN,
            });
        }
        if !candidate
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(SlugError::InvalidCharacter);
        }
        if candidate.starts_with('-') || candidate.ends_with('-') {
            return Err(SlugError::LeadingOrTrailingHyphen);
        }
        if candidate.contains("--") {
            return Err(SlugError::ConsecutiveHyphens);
        }
        Ok(())
    }

    /// Normalises human input into a valid slug.
    ///
    /// Lowercases, turns any run of non-alphanumeric characters into a single
    /// hyphen, and trims hyphens from the ends. A name like `"Payments API (EU)"`
    /// becomes `"payments-api-eu"` — which is what an operator meant.
    ///
    /// A result longer than [`Slug::MAX_LEN`] is truncated rather than refused:
    /// the slug is a derived identifier, so an over-long display name is a
    /// naming choice and failing the whole create over it would be unhelpful.
    /// Truncation is the *only* length adjustment, because silently dropping
    /// characters is safe here precisely because the human-readable `name`
    /// carries the full text.
    ///
    /// Still returns an error when the result is unusable — an all-punctuation
    /// name, or one that collapses to less than [`Slug::MIN_LEN`] — so a caller
    /// can never construct a `Slug` that does not satisfy [`Slug::validate`].
    pub fn parse(input: &str) -> Result<Self, SlugError> {
        let lowered = input.trim().to_ascii_lowercase();

        let mut out = String::with_capacity(lowered.len());
        let mut last_was_hyphen = true; // suppresses a leading hyphen
        for ch in lowered.chars() {
            if ch.is_ascii_alphanumeric() {
                out.push(ch);
                last_was_hyphen = false;
            } else if !last_was_hyphen {
                out.push('-');
                last_was_hyphen = true;
            }
        }
        while out.ends_with('-') {
            out.pop();
        }

        if out.len() > Self::MAX_LEN {
            out.truncate(Self::MAX_LEN);
            // Truncation can land on a hyphen (`payments-api-eu-lo` → cut after
            // a separator), which `validate` rejects; dropping it keeps the
            // result inside the same rules.
            while out.ends_with('-') {
                out.pop();
            }
        }

        Self::try_from(out)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Slug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for Slug {
    type Error = SlugError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::validate(&value)?;
        Ok(Self(value))
    }
}

impl TryFrom<&str> for Slug {
    type Error = SlugError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::validate(value)?;
        Ok(Self(value.to_owned()))
    }
}

impl From<Slug> for String {
    fn from(value: Slug) -> Self {
        value.0
    }
}

impl AsRef<str> for Slug {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Why a candidate slug was rejected.
///
/// Distinct variants rather than one message, because "use lowercase letters,
/// numbers and hyphens" is actionable and "invalid slug" is not.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SlugError {
    #[error("a slug must be at least {min} characters, got {len}")]
    TooShort { len: usize, min: usize },
    #[error("a slug must be at most {max} characters, got {len}")]
    TooLong { len: usize, max: usize },
    #[error("a slug may only contain lowercase letters, digits and hyphens")]
    InvalidCharacter,
    #[error("a slug must not start or end with a hyphen")]
    LeadingOrTrailingHyphen,
    #[error("a slug must not contain consecutive hyphens")]
    ConsecutiveHyphens,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_normal_slug_is_accepted() {
        assert!(Slug::validate("payments").is_ok());
        assert!(Slug::validate("payments-api").is_ok());
        assert!(Slug::validate("p4y-2").is_ok());
    }

    #[test]
    fn slugs_are_case_sensitive_in_a_way_that_would_confuse() {
        // Uppercase is rejected rather than silently lowercased: a slug is often
        // typed by hand, and accepting "Payments" here would let two slugs that
        // differ only in case reach the database.
        assert_eq!(
            Slug::validate("Payments"),
            Err(SlugError::InvalidCharacter)
        );
    }

    #[test]
    fn boundaries_are_enforced() {
        assert!(Slug::validate("a").is_err(), "one char is too short");
        assert!(Slug::validate("ab").is_ok(), "two chars is the minimum");
        let max = "a".repeat(Slug::MAX_LEN);
        assert!(Slug::validate(&max).is_ok());
        assert!(Slug::validate(&format!("{max}a")).is_err());
    }

    #[test]
    fn hyphens_are_constrained_at_both_ends_and_in_the_middle() {
        assert_eq!(
            Slug::validate("-payments"),
            Err(SlugError::LeadingOrTrailingHyphen)
        );
        assert_eq!(
            Slug::validate("payments-"),
            Err(SlugError::LeadingOrTrailingHyphen)
        );
        assert_eq!(
            Slug::validate("payments--api"),
            Err(SlugError::ConsecutiveHyphens)
        );
    }

    #[test]
    fn punctuation_and_spaces_are_rejected() {
        assert_eq!(
            Slug::validate("payments api"),
            Err(SlugError::InvalidCharacter)
        );
        assert_eq!(
            Slug::validate("payments_api"),
            Err(SlugError::InvalidCharacter)
        );
        assert_eq!(
            Slug::validate("payments/api"),
            Err(SlugError::InvalidCharacter)
        );
    }

    #[test]
    fn parse_normalises_human_input() {
        assert_eq!(Slug::parse("Payments API").unwrap().as_str(), "payments-api");
        assert_eq!(
            Slug::parse("  Payments (EU)  ").unwrap().as_str(),
            "payments-eu"
        );
        assert_eq!(
            Slug::parse("nightly_settlement_v2").unwrap().as_str(),
            "nightly-settlement-v2"
        );
    }

    #[test]
    fn parse_collapses_runs_of_separators_into_one_hyphen() {
        // The whole point of the consecutive-hyphen rule: a display name with
        // punctuation must not produce a slug the validator rejects.
        assert_eq!(
            Slug::parse("a  --  b").unwrap().as_str(),
            "a-b"
        );
    }

    #[test]
    fn parse_trims_leading_and_trailing_separators() {
        assert_eq!(Slug::parse("---payments---").unwrap().as_str(), "payments");
    }

    #[test]
    fn parse_still_rejects_input_that_normalises_to_nothing_usable() {
        // Silently producing an empty or one-character slug would create an
        // addressable resource with an unusable name.
        assert!(Slug::parse("!!!").is_err());
        assert!(Slug::parse("   ").is_err());
        assert!(Slug::parse("a").is_err());
        assert!(Slug::parse("").is_err());
    }

    #[test]
    fn parse_never_produces_a_slug_that_validate_would_reject() {
        // The invariant that makes `parse` safe to use at an edge: whatever it
        // returns already satisfies the strict rules.
        for candidate in [
            "Payments",
            "payments api",
            "--payments--",
            "ACME Payments (EU) / Core",
            "payments-api",
            "a1",
        ] {
            if let Ok(slug) = Slug::parse(candidate) {
                assert!(
                    Slug::validate(slug.as_str()).is_ok(),
                    "parse produced an invalid slug from {candidate:?}: {slug}"
                );
            }
        }
    }

    #[test]
    fn parse_truncates_over_long_names_rather_than_failing() {
        // A long display name is a naming choice, not an error; the slug is a
        // derived identifier, so cutting it is friendlier than refusing.
        let long = "extremely long application name that will not fit";
        let slug = Slug::parse(long).unwrap();
        assert!(slug.as_str().len() <= Slug::MAX_LEN);
        assert!(Slug::validate(slug.as_str()).is_ok());
    }

    #[test]
    fn slugs_round_trip_through_serde_as_their_string() {
        let slug = Slug::parse("payments").unwrap();
        let json = serde_json::to_string(&slug).unwrap();
        assert_eq!(json, "\"payments\"");
        let back: Slug = serde_json::from_str(&json).unwrap();
        assert_eq!(back, slug);
    }

    #[test]
    fn deserialising_an_invalid_slug_fails_rather_than_constructing_one() {
        assert!(serde_json::from_str::<Slug>("\"Not A Slug\"").is_err());
    }
}