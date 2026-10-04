//! Applications and environments: the two containers a job belongs to.
//!
//! `redesign.md` asks for PowerJob-style application grouping (§D) and for
//! multi-environment operation (§2.G, §3). Neither existed: a job's only
//! grouping was a queue plus a bag of labels, and there was no environment to
//! migrate between.
//!
//! The split between them matters and is the reason they are separate types
//! rather than one "group" concept:
//!
//! - An **application** is *what* the work is. "Payments", "Fraud scoring".
//!   It travels unchanged between environments.
//! - An **environment** is *where* it runs. "dev", "staging", "prod". It decides
//!   which workers, queues, and secrets a job may reach.
//!
//! Migration copies the first and re-points the second, which is only
//! expressible if they are independently addressable.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::id::{ApplicationId, EnvironmentId, Slug, TenantId};

/// A grouping of related jobs and workers within one tenant.
///
/// Modelled on PowerJob's `appId`, which is the idea worth copying: a platform
/// serving several products needs a container that answers "which application's
/// jobs are failing?" — a question a queue name and a JSON label bag cannot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Application {
    pub id: ApplicationId,
    pub tenant_id: TenantId,
    /// Stable, typeable identifier, unique within the tenant.
    pub slug: Slug,
    pub name: String,
    pub description: Option<String>,
    /// Free-form operator labels. Deliberately not an authorisation input:
    /// nothing reads them for access control.
    #[serde(default)]
    pub labels: serde_json::Map<String, serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// What an environment is for.
///
/// Modelled as a closed set rather than a free string because the *behaviour*
/// differs per kind, and free strings would let a deployment end up with
/// "production", "prod" and "PRODUCTION" as three environments that an operator
/// reasonably believes are isolated but are not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentKind {
    /// Local or CI-run work.
    Development,
    /// Verifies the production shape before promotion.
    Staging,
    /// Customer-facing. Requires the production guardrails to change anything.
    Production,
    /// A customer's own deployment, or a short-lived sandbox.
    Other,
}

impl EnvironmentKind {
    /// Whether destructive or irreversible changes need explicit confirmation.
    ///
    /// This is what the console's production guardrail keys off, so the rule
    /// lives in the domain rather than in whichever screen happens to render it.
    pub fn is_protected(self) -> bool {
        matches!(self, EnvironmentKind::Production)
    }

    /// Whether scheduling may be paused for everyone at once.
    pub fn allows_maintenance_mode(self) -> bool {
        !matches!(self, EnvironmentKind::Production)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            EnvironmentKind::Development => "development",
            EnvironmentKind::Staging => "staging",
            EnvironmentKind::Production => "production",
            EnvironmentKind::Other => "other",
        }
    }
}

impl std::str::FromStr for EnvironmentKind {
    type Err = EnvironmentError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "development" | "dev" | "local" => Ok(EnvironmentKind::Development),
            "staging" | "stage" | "uat" => Ok(EnvironmentKind::Staging),
            "production" | "prod" => Ok(EnvironmentKind::Production),
            "other" | "sandbox" | "customer" => Ok(EnvironmentKind::Other),
            other => Err(EnvironmentError::UnknownKind(other.to_string())),
        }
    }
}

impl std::fmt::Display for EnvironmentKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A deployment environment: the unit that workers, queues and secrets belong
/// to, and the axis a migration moves a job along.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Environment {
    pub id: EnvironmentId,
    pub tenant_id: TenantId,
    pub slug: Slug,
    pub name: String,
    pub kind: EnvironmentKind,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Environment {
    /// Whether a change in this environment needs production-grade friction.
    ///
    /// Reads the environment's own kind rather than accepting a flag from the
    /// caller, so a client cannot downgrade a production guardrail by asking
    /// nicely.
    pub fn is_protected(&self) -> bool {
        self.kind.is_protected()
    }
}

/// Why an environment could not be created or changed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EnvironmentError {
    #[error(
        "`{0}` is not a known environment kind (use development, staging, production or other)"
    )]
    UnknownKind(String),

    #[error("a tenant may have at most one production environment")]
    DuplicateProduction,

    #[error("the environment slug is already in use within this tenant")]
    SlugTaken,

    #[error("an environment name is required")]
    NameRequired,

    #[error("the environment `{0}` does not exist in this tenant")]
    NotFound(String),
}

/// Which half of a job a migration copies.
///
/// The split is the mechanism that makes migration safe. A job's identity — its
/// name, key, schedule, payload shape — is the same in dev and prod; its
/// bindings — which queue, which worker pool, which secret names — are not, and
/// copying them verbatim would point production at a dev credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum MigratableField {
    /// Name, key, description, priority, labels, retry and timeout policy.
    Identity,
    /// The schedule: expression, timezone, misfire policy, catch-up limit.
    Schedule,
    /// The job's input payload shape and parameter names.
    Parameters,
    /// Queue, worker requirements, and referenced secret names.
    ///
    /// Included by request only, and never by default: these name things that
    /// do not exist in the destination.
    Bindings,
}

impl MigratableField {
    /// Whether this field can be copied without the destination resolving names.
    ///
    /// Bindings always require an explicit mapping step, which is why they are
    /// opt-in — a migration that silently included them would produce a job
    /// pointing at a `dev-payments-queue` that production does not have.
    pub fn needs_resolution(self) -> bool {
        matches!(self, MigratableField::Bindings)
    }
}

/// What a migration will do, before it does it.
///
/// `redesign.md` §3 lists "change previews" and "controlled rollouts" as a
/// differentiator, and 14-testing-strategy is explicit that nothing applies
/// silently. Both need the same artifact: a computed plan the operator reads and
/// confirms.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MigrationPlan {
    pub source_tenant: Slug,
    pub source_environment: Slug,
    pub target_tenant: Slug,
    pub target_environment: Slug,
    pub application: Option<Slug>,
    /// Every job the plan would carry across, in a stable order.
    pub entries: Vec<MigrationEntry>,
    pub include: Vec<MigratableField>,
}

/// One job's proposed crossing, and the conflicts a human must resolve.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MigrationEntry {
    /// Stable identifier *within the source system*, used to match a job on the
    /// way back. Never the database id: those differ between deployments.
    pub source_key: String,
    pub name: String,
    /// What would happen to this job in the destination.
    pub action: MigrationAction,
    /// Fields that cannot be copied as-is, each with the reason.
    pub unresolved: Vec<UnresolvedBinding>,
}

/// What a migration does with one job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationAction {
    /// No job with this key exists in the destination.
    Create,
    /// A job with this key exists; the plan would update it.
    Update,
    /// A job with this key exists and is byte-identical, so nothing changes.
    Unchanged,
}

/// A binding the destination cannot resolve on its own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnresolvedBinding {
    /// Which side of the job this is: a queue, a worker pool, a secret.
    pub kind: BindingKind,
    /// The name used in the source.
    pub source_name: String,
    /// Why it cannot be carried across.
    pub reason: UnresolvedReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingKind {
    Queue,
    WorkerPool,
    Secret,
}

/// Why a binding needs a human decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum UnresolvedReason {
    /// No object of this kind with that name exists in the destination.
    MissingInTarget,
    /// An object exists but carries different configuration, so copying the name
    /// would silently retarget the job at something else.
    ConfigurationDiffers,
    /// The object exists in the source but is scoped to another tenant.
    ForeignTenant,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_is_the_only_protected_kind() {
        assert!(EnvironmentKind::Production.is_protected());
        assert!(!EnvironmentKind::Staging.is_protected());
        assert!(!EnvironmentKind::Development.is_protected());
        assert!(!EnvironmentKind::Other.is_protected());
    }

    #[test]
    fn production_refuses_maintenance_mode() {
        // Pausing scheduling for everyone at once is an operational tool for
        // maintenance windows; in production it would silently stop customer
        // workloads, so the domain refuses it.
        assert!(!EnvironmentKind::Production.allows_maintenance_mode());
        assert!(EnvironmentKind::Staging.allows_maintenance_mode());
        assert!(EnvironmentKind::Development.allows_maintenance_mode());
    }

    #[test]
    fn environment_kinds_parse_from_the_names_operators_actually_type() {
        use std::str::FromStr;
        assert_eq!(
            EnvironmentKind::from_str("prod").unwrap(),
            EnvironmentKind::Production
        );
        assert_eq!(
            EnvironmentKind::from_str("PROD").unwrap(),
            EnvironmentKind::Production
        );
        assert_eq!(
            EnvironmentKind::from_str(" dev ").unwrap(),
            EnvironmentKind::Development
        );
        assert_eq!(
            EnvironmentKind::from_str("UAT").unwrap(),
            EnvironmentKind::Staging
        );
    }

    #[test]
    fn an_unknown_kind_is_rejected_rather_than_defaulted() {
        // Defaulting an unrecognised name to Production would flag harmless
        // environments as protected; defaulting to Development would let an
        // operator create an unguarded "PRODUCTION-1" by accident.
        use std::str::FromStr;
        let err = EnvironmentKind::from_str("production-2").unwrap_err();
        assert_eq!(
            err,
            EnvironmentError::UnknownKind("production-2".to_string())
        );
    }

    #[test]
    fn a_kind_round_trips_through_its_display_form() {
        use std::str::FromStr;
        for kind in [
            EnvironmentKind::Development,
            EnvironmentKind::Staging,
            EnvironmentKind::Production,
            EnvironmentKind::Other,
        ] {
            assert_eq!(EnvironmentKind::from_str(kind.as_str()).unwrap(), kind);
        }
    }

    #[test]
    fn bindings_are_the_only_field_needing_resolution() {
        // This is the invariant that makes a default migration safe: identity,
        // schedule and parameters are copyable verbatim, and bindings — the
        // only ones that name destination objects — are opt-in.
        assert!(!MigratableField::Identity.needs_resolution());
        assert!(!MigratableField::Schedule.needs_resolution());
        assert!(!MigratableField::Parameters.needs_resolution());
        assert!(MigratableField::Bindings.needs_resolution());
    }

    #[test]
    fn a_migration_plan_round_trips_through_json() {
        // The plan is reviewed in the console before it is applied, so it has
        // to survive the wire in both directions without losing a variant.
        let plan = MigrationPlan {
            source_tenant: Slug::parse("acme").unwrap(),
            source_environment: Slug::parse("dev").unwrap(),
            target_tenant: Slug::parse("acme").unwrap(),
            target_environment: Slug::parse("prod").unwrap(),
            application: Some(Slug::parse("payments").unwrap()),
            include: vec![MigratableField::Identity, MigratableField::Schedule],
            entries: vec![
                MigrationEntry {
                    source_key: "nightly-settlement".into(),
                    name: "Nightly settlement".into(),
                    action: MigrationAction::Create,
                    unresolved: vec![UnresolvedBinding {
                        kind: BindingKind::Queue,
                        source_name: "dev-payments".into(),
                        reason: UnresolvedReason::MissingInTarget,
                    }],
                },
                MigrationEntry {
                    source_key: "dunning".into(),
                    name: "Dunning".into(),
                    action: MigrationAction::Update,
                    unresolved: vec![],
                },
            ],
        };

        let json = serde_json::to_string(&plan).unwrap();
        let back: MigrationPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(back, plan);
    }

    #[test]
    fn the_unresolved_reason_keeps_its_tag_when_serialised() {
        // Internally tagged enums silently lose their discriminator if it is
        // dropped, turning three distinct reasons into one unreadable object.
        let binding = UnresolvedBinding {
            kind: BindingKind::Secret,
            source_name: "PSP_API_KEY".into(),
            reason: UnresolvedReason::ForeignTenant,
        };
        let json = serde_json::to_value(&binding).unwrap();
        assert_eq!(json["reason"]["reason"], "foreign_tenant");
    }
}
