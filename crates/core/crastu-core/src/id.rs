use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! id_type {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// A new time-ordered id (UUID v7), so ids sort by creation time.
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            pub fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
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
                self.0.fmt(f)
            }
        }

        impl std::str::FromStr for $name {
            type Err = uuid::Error;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(s).map(Self)
            }
        }
    };
}

id_type!(
    /// Identifies a project.
    ProjectId
);
id_type!(
    /// Identifies an imported or generated asset.
    AssetId
);
id_type!(
    /// Identifies a background job.
    JobId
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_through_strings_and_json() {
        let id = ProjectId::new();
        let parsed: ProjectId = id.to_string().parse().unwrap();
        assert_eq!(id, parsed);
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, format!("\"{id}\""));
    }

    #[test]
    fn ids_sort_by_creation() {
        let a = JobId::new();
        let b = JobId::new();
        assert!(a < b);
    }
}
