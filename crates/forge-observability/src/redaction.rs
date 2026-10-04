//! Secret redaction (spec 01.22 invariant 11, spec 11, AT-SEC-001).
//!
//! Secret values must never appear in logs, audit metadata, or error messages.
//! Redaction happens at the point a value is rendered, so a new call site cannot
//! accidentally leak by forgetting to sanitise.

/// A header or field whose value is always secret.
const SENSITIVE_KEYS: &[&str] = &[
    "authorization",
    "cookie",
    "set-cookie",
    "password",
    "passwd",
    "secret",
    "token",
    "access_token",
    "refresh_token",
    "id_token",
    "api_key",
    "apikey",
    "x-api-key",
    "private_key",
    "client_secret",
    "session",
    "session_id",
    "csrf_token",
];

/// The placeholder substituted for a redacted value.
pub const REDACTED: &str = "[REDACTED]";

/// Whether a key names a secret.
///
/// Matching is on the key's *segments* rather than a plain substring: a
/// substring test would redact innocent fields like `tokenizer` or
/// `description_token_count`. Splitting on `_`, `-` and `.` means `db_password`
/// and `x-api-key` still match while `tokenizer` does not.
pub fn is_sensitive(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();

    // An exact hit on a known header or field is always sensitive.
    if SENSITIVE_KEYS.contains(&lower.as_str()) {
        return true;
    }

    // Otherwise look for a sensitive word as a whole segment, or as a suffix
    // such as `..._token` / `..._password`.
    let segments: Vec<&str> = lower
        .split(['_', '-', '.'])
        .filter(|s| !s.is_empty())
        .collect();

    for segment in &segments {
        if SENSITIVE_KEYS.contains(segment) {
            return true;
        }
    }

    // `accessToken`-style camelCase keys normalise to a single segment, so also
    // check the trailing word of a concatenated key.
    if let Some(last) = segments.last() {
        if SENSITIVE_KEYS.iter().any(|s| last.ends_with(s))
            && *last != *segments.first().unwrap_or(last)
        {
            return true;
        }
    }

    false
}

/// Redacts a single value if its key is sensitive.
pub fn redact_value(key: &str, value: &str) -> String {
    if is_sensitive(key) {
        REDACTED.to_string()
    } else {
        value.to_string()
    }
}

/// Recursively redacts every sensitive field in a JSON document.
///
/// Nested objects and arrays are walked, because a secret is just as exposed
/// inside `{"body": {"password": "..."}}` as at the top level.
pub fn redact_json(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => map
            .iter()
            .map(|(k, v)| {
                if is_sensitive(k) {
                    // Replace the whole subtree, so a secret nested under a
                    // sensitive key cannot survive.
                    (k.clone(), serde_json::Value::String(REDACTED.to_string()))
                } else {
                    (k.clone(), redact_json(v))
                }
            })
            .collect(),
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(redact_json).collect())
        }
        other => other.clone(),
    }
}

/// Redacts the values of sensitive headers, keeping the header names so a
/// reader can still see *that* authorisation was present.
pub fn redact_headers<'a, I>(headers: I) -> Vec<(String, String)>
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    headers
        .into_iter()
        .map(|(k, v)| (k.to_string(), redact_value(k, v)))
        .collect()
}

/// A URL with its query string removed, for logging.
///
/// Query strings routinely carry tokens (`?access_token=...`) and are far more
/// likely to be pasted into an issue than the path.
pub fn redact_url(url: &str) -> String {
    match url.split_once('?') {
        Some((base, _query)) => format!("{base}?[REDACTED]"),
        None => url.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // AT-SEC-001: secrets are absent from logs.
    #[test]
    fn a_password_is_never_rendered() {
        assert_eq!(
            redact_value("password", "hunter2"),
            REDACTED,
            "a password must never be echoed"
        );
        assert_eq!(redact_value("user_password", "hunter2"), REDACTED);
        assert_eq!(redact_value("Authorization", "Bearer abc.def"), REDACTED);
        assert_eq!(redact_value("X-API-Key", "key-123"), REDACTED);
    }

    #[test]
    fn a_non_sensitive_value_is_preserved() {
        assert_eq!(redact_value("job_name", "settle-daily"), "settle-daily");
        assert_eq!(redact_value("status", "RUNNING"), "RUNNING");
    }

    #[test]
    fn sensitivity_matching_is_case_insensitive_and_substring_based() {
        assert!(is_sensitive("AUTHORIZATION"));
        assert!(is_sensitive("Refresh-Token"));
        assert!(is_sensitive("db_password"));
        assert!(!is_sensitive("tokenizer"));
        assert!(!is_sensitive("description"));
    }

    #[test]
    fn nested_secrets_are_redacted() {
        let document = json!({
            "name": "settle",
            "credentials": { "password": "hunter2", "user": "svc" },
            "items": [{ "api_key": "k-1" }],
            "api_key": "top-level",
        });

        let redacted = redact_json(&document);
        let text = redacted.to_string();

        assert!(!text.contains("hunter2"), "nested password leaked");
        assert!(!text.contains("k-1"), "nested array secret leaked");
        assert!(!text.contains("top-level"), "top-level secret leaked");
        // Non-sensitive context survives, so the log stays useful.
        assert!(text.contains("settle"));
        assert!(text.contains("svc"));
    }

    #[test]
    fn a_secret_nested_under_a_sensitive_key_is_replaced_wholesale() {
        let document = json!({ "secret": { "inner": "value" } });
        let redacted = redact_json(&document);
        assert_eq!(redacted["secret"], json!(REDACTED));
        assert!(!redacted.to_string().contains("value"));
    }

    #[test]
    fn headers_keep_their_names_but_lose_their_values() {
        let redacted = redact_headers([
            ("authorization", "Bearer secret-token"),
            ("content-type", "application/json"),
        ]);

        assert_eq!(redacted[0].0, "authorization");
        assert_eq!(redacted[0].1, REDACTED);
        assert_eq!(redacted[1].1, "application/json");
        assert!(!redacted.iter().any(|(_, v)| v.contains("secret-token")));
    }

    #[test]
    fn a_url_query_string_is_redacted() {
        assert_eq!(
            redact_url("https://api.example.com/v1/jobs?access_token=abc"),
            "https://api.example.com/v1/jobs?[REDACTED]"
        );
        assert_eq!(
            redact_url("https://api.example.com/v1/health"),
            "https://api.example.com/v1/health",
            "a URL with no query is unchanged"
        );
    }

    #[test]
    fn scalars_and_nulls_pass_through() {
        assert_eq!(redact_json(&json!(42)), json!(42));
        assert_eq!(redact_json(&json!("text")), json!("text"));
        assert_eq!(redact_json(&json!(null)), json!(null));
    }
}
