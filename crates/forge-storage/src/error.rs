use thiserror::Error;

/// Errors surfaced by the repository layer.
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("{entity} not found")]
    NotFound { entity: &'static str },

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("validation error: {0}")]
    Validation(String),

    /// Spec 01.16 invariant 1 / spec 08.3: a tenant may not reach another
    /// tenant's rows. Reported as a not-found so the error itself does not
    /// leak the existence of the other tenant's resource (AT-TEN-003).
    #[error("tenant isolation violation: {0}")]
    TenantIsolation(String),

    #[error("invalid cursor: {0}")]
    InvalidCursor(String),
}

impl StorageError {
    pub fn not_found(entity: &'static str) -> Self {
        StorageError::NotFound { entity }
    }

    /// Maps a unique-violation to a domain-level conflict.
    ///
    /// PostgreSQL error code 23505.
    pub fn from_sqlx(err: sqlx::Error) -> Self {
        if let sqlx::Error::Database(ref db_err) = err {
            if db_err.code().as_deref() == Some("23505") {
                return StorageError::Conflict("unique constraint violated".to_string());
            }
            if db_err.code().as_deref() == Some("23503") {
                return StorageError::Conflict("referenced resource does not exist".to_string());
            }
        }
        StorageError::Database(err)
    }
}

pub type Result<T> = std::result::Result<T, StorageError>;

/// Opaque cursor for spec 05 §5.14 cursor pagination.
///
/// The encoding is deliberately opaque to clients: it carries the ordering key
/// and the last row's identity so a subsequent page can resume deterministically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    /// Value of the sort column on the last row of the previous page.
    pub sort_value: String,
    /// Tiebreaker identity, so equal sort values still page deterministically.
    pub last_id: uuid::Uuid,
}

impl Cursor {
    pub fn new(sort_value: impl Into<String>, last_id: uuid::Uuid) -> Self {
        Self {
            sort_value: sort_value.into(),
            last_id,
        }
    }

    /// Encodes the cursor as an opaque URL-safe token.
    pub fn encode(&self) -> String {
        use base64::Engine as _;
        let raw = format!("{}|{}", self.sort_value, self.last_id);
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw)
    }

    pub fn decode(token: &str) -> Result<Self> {
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(token)
            .map_err(|_| StorageError::InvalidCursor("not valid base64".to_string()))?;
        let raw = String::from_utf8(bytes)
            .map_err(|_| StorageError::InvalidCursor("not valid UTF-8".to_string()))?;
        let (sort_value, id) = raw
            .rsplit_once('|')
            .ok_or_else(|| StorageError::InvalidCursor("malformed cursor".to_string()))?;
        let last_id = uuid::Uuid::parse_str(id)
            .map_err(|_| StorageError::InvalidCursor("malformed cursor id".to_string()))?;
        Ok(Cursor {
            sort_value: sort_value.to_string(),
            last_id,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

impl<T> Page<T> {
    /// Builds a page from one extra row fetched beyond `limit`, which is how
    /// `has_more` is determined without a second COUNT query.
    pub fn from_overfetch(
        mut rows: Vec<T>,
        limit: usize,
        to_cursor: impl Fn(&T) -> Cursor,
    ) -> Self {
        let has_more = rows.len() > limit;
        if has_more {
            rows.truncate(limit);
        }
        let next_cursor = if has_more {
            rows.last().map(|last| to_cursor(last).encode())
        } else {
            None
        };
        Self {
            items: rows,
            next_cursor,
            has_more,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_round_trips() {
        let id = uuid::Uuid::new_v4();
        let cursor = Cursor::new("2026-10-03T09:00:00Z", id);
        let decoded = Cursor::decode(&cursor.encode()).unwrap();
        assert_eq!(decoded, cursor);
    }

    #[test]
    fn cursor_handles_values_containing_pipes() {
        let id = uuid::Uuid::new_v4();
        // rsplit_once on the final '|' keeps pipes inside the sort value intact.
        let cursor = Cursor::new("a|b|c", id);
        let decoded = Cursor::decode(&cursor.encode()).unwrap();
        assert_eq!(decoded.sort_value, "a|b|c");
        assert_eq!(decoded.last_id, id);
    }

    #[test]
    fn malformed_cursors_are_rejected() {
        assert!(Cursor::decode("not base64!!").is_err());
        assert!(Cursor::decode(&base64::Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            "no-separator"
        ))
        .is_err());
    }

    #[test]
    fn page_reports_more_when_overfetched() {
        let rows: Vec<(String, uuid::Uuid)> = (0..5)
            .map(|i| (format!("row{i}"), uuid::Uuid::new_v4()))
            .collect();
        let page =
            Page::from_overfetch(rows.clone(), 3, |(name, id)| Cursor::new(name.clone(), *id));

        assert_eq!(page.items.len(), 3, "limit is enforced");
        assert!(page.has_more);
        assert!(page.next_cursor.is_some());

        // The cursor points at the last retained row, not the discarded one.
        let decoded = Cursor::decode(page.next_cursor.as_ref().unwrap()).unwrap();
        assert_eq!(decoded.sort_value, "row2");
    }

    #[test]
    fn page_reports_no_more_when_exactly_filled() {
        let rows: Vec<(String, uuid::Uuid)> = (0..3)
            .map(|i| (format!("row{i}"), uuid::Uuid::new_v4()))
            .collect();
        let page = Page::from_overfetch(rows, 3, |(name, id)| Cursor::new(name.clone(), *id));
        assert_eq!(page.items.len(), 3);
        assert!(!page.has_more);
        assert!(page.next_cursor.is_none());
    }

    #[test]
    fn unique_violation_maps_to_conflict() {
        use sqlx::error::{DatabaseError, ErrorKind};
        use std::borrow::Cow;

        /// Minimal driver-error stand-in carrying PostgreSQL's SQLSTATE, which
        /// is what `from_sqlx` keys off.
        #[derive(Debug)]
        struct FakeDbError(&'static str);

        impl std::fmt::Display for FakeDbError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.0)
            }
        }

        impl std::error::Error for FakeDbError {}

        impl DatabaseError for FakeDbError {
            fn message(&self) -> &str {
                "duplicate key value violates unique constraint"
            }
            fn code(&self) -> Option<Cow<'_, str>> {
                Some(Cow::Borrowed(self.0))
            }
            fn kind(&self) -> ErrorKind {
                ErrorKind::UniqueViolation
            }
            fn as_error(&self) -> &(dyn std::error::Error + Send + Sync + 'static) {
                self
            }
            fn as_error_mut(&mut self) -> &mut (dyn std::error::Error + Send + Sync + 'static) {
                self
            }
            fn into_error(self: Box<Self>) -> Box<dyn std::error::Error + Send + Sync + 'static> {
                self
            }
        }

        let err = sqlx::Error::Database(Box::new(FakeDbError("23505")));
        assert!(matches!(
            StorageError::from_sqlx(err),
            StorageError::Conflict(_)
        ));

        // A foreign-key violation maps to a conflict too.
        let fk = sqlx::Error::Database(Box::new(FakeDbError("23503")));
        assert!(matches!(
            StorageError::from_sqlx(fk),
            StorageError::Conflict(_)
        ));

        // An unrelated code stays a database error.
        let other = sqlx::Error::Database(Box::new(FakeDbError("42P01")));
        assert!(matches!(
            StorageError::from_sqlx(other),
            StorageError::Database(_)
        ));
    }
}
