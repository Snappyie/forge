use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::fmt;

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
