//! Typed Helix Cloud resources as WFE returns them (proto3 JSON, camelCase).
//!
//! Only `id` is required; every other field defaults when absent. Fields the
//! CLI does not model are kept in `extra`, so `--json` output never drops
//! anything the server returned.

use crate::config::DatabaseReference;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// A resource that can be matched by ID, slug, or display name.
pub trait Named {
    fn id(&self) -> &str;
    fn slug(&self) -> &str;
    fn display_name(&self) -> &str;

    /// The value to pass on the command line to select exactly this resource.
    fn argument(&self) -> String {
        self.id().to_owned()
    }

    /// The friendliest non-empty identifier: display name, then slug, then ID.
    fn label(&self) -> &str {
        [self.display_name(), self.slug()]
            .into_iter()
            .find(|value| !value.is_empty())
            .unwrap_or_else(|| self.id())
    }
}

/// Human form of a `RESOURCE_STATUS_*` value: `RESOURCE_STATUS_ACTIVE` → `active`.
///
/// ```
/// use helix_cli::cloud::model::status_label;
///
/// assert_eq!(status_label("RESOURCE_STATUS_ACTIVE"), "active");
/// assert_eq!(status_label("ready"), "ready");
/// assert_eq!(status_label(""), "unknown");
/// ```
pub fn status_label(status: &str) -> String {
    match status.strip_prefix("RESOURCE_STATUS_").unwrap_or(status) {
        "" | "UNSPECIFIED" => "unknown".to_owned(),
        status => status.to_lowercase(),
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub status: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    #[serde(default)]
    pub workspace_id: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Whether a cluster is dedicated to one project (and so is itself a
/// database target) or shared between tenant databases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
pub enum ClusterAccess {
    #[serde(rename = "CLUSTER_ACCESS_DEDICATED", alias = "dedicated")]
    Dedicated,
    #[serde(rename = "CLUSTER_ACCESS_SHARED", alias = "shared")]
    Shared,
    #[default]
    #[serde(rename = "CLUSTER_ACCESS_UNSPECIFIED", other)]
    Unspecified,
}

impl ClusterAccess {
    pub fn label(self) -> &'static str {
        match self {
            Self::Dedicated => "dedicated",
            Self::Shared => "shared",
            Self::Unspecified => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cluster {
    pub id: String,
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub workspace_id: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub access: ClusterAccess,
    #[serde(default)]
    pub status: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tenant {
    pub id: String,
    #[serde(default)]
    pub cluster_id: String,
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub workspace_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub status: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A queryable database: a dedicated cluster or a tenant.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Database {
    Dedicated(Cluster),
    Tenant(Tenant),
}

impl Database {
    pub fn reference(&self) -> DatabaseReference {
        match self {
            Self::Dedicated(cluster) => DatabaseReference::Cluster(cluster.id.clone()),
            Self::Tenant(tenant) => DatabaseReference::Tenant(tenant.id.clone()),
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Dedicated(_) => "dedicated",
            Self::Tenant(_) => "tenant",
        }
    }

    pub fn status(&self) -> &str {
        match self {
            Self::Dedicated(cluster) => &cluster.status,
            Self::Tenant(tenant) => &tenant.status,
        }
    }

    pub fn project_id(&self) -> &str {
        match self {
            Self::Dedicated(cluster) => &cluster.project_id,
            Self::Tenant(tenant) => &tenant.project_id,
        }
    }

    pub fn workspace_id(&self) -> &str {
        match self {
            Self::Dedicated(cluster) => &cluster.workspace_id,
            Self::Tenant(tenant) => &tenant.workspace_id,
        }
    }
}

/// An application key for a database. Its token is only ever returned once,
/// at creation, and is never part of this model.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseKey {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub access: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl DatabaseKey {
    /// `read-write` / `read-only` from the access enum, else the raw permissions.
    pub fn access_label(&self) -> String {
        match self.access.strip_prefix("DATABASE_KEY_ACCESS_") {
            Some(access) => access.to_lowercase().replace('_', "-"),
            None if !self.access.is_empty() => self.access.clone(),
            None => self.permissions.join(","),
        }
    }
}

/// A workspace-owned headless credential. Its secret is never returned after
/// creation and is never part of this model.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceCredential {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub grants: Vec<Value>,
    #[serde(default)]
    pub expires_at: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Named for Workspace {
    fn id(&self) -> &str {
        &self.id
    }
    fn slug(&self) -> &str {
        &self.slug
    }
    fn display_name(&self) -> &str {
        &self.display_name
    }
}

impl Named for Project {
    fn id(&self) -> &str {
        &self.id
    }
    fn slug(&self) -> &str {
        &self.slug
    }
    fn display_name(&self) -> &str {
        &self.display_name
    }
}

impl Named for Cluster {
    fn id(&self) -> &str {
        &self.id
    }
    fn slug(&self) -> &str {
        &self.slug
    }
    fn display_name(&self) -> &str {
        &self.display_name
    }
}

impl Named for Tenant {
    fn id(&self) -> &str {
        &self.id
    }
    fn slug(&self) -> &str {
        &self.slug
    }
    /// Tenants carry a `name`; older responses use `displayName`.
    fn display_name(&self) -> &str {
        if self.name.is_empty() {
            &self.display_name
        } else {
            &self.name
        }
    }
}

impl Named for Database {
    /// The typed `tenant:<id>` / `cluster:<id>` reference, which resolves
    /// without any project context.
    fn argument(&self) -> String {
        self.reference().to_string()
    }
    fn id(&self) -> &str {
        match self {
            Self::Dedicated(cluster) => cluster.id(),
            Self::Tenant(tenant) => tenant.id(),
        }
    }
    fn slug(&self) -> &str {
        match self {
            Self::Dedicated(cluster) => cluster.slug(),
            Self::Tenant(tenant) => tenant.slug(),
        }
    }
    fn display_name(&self) -> &str {
        match self {
            Self::Dedicated(cluster) => cluster.display_name(),
            Self::Tenant(tenant) => tenant.display_name(),
        }
    }
}

impl Named for DatabaseKey {
    fn id(&self) -> &str {
        &self.id
    }
    fn slug(&self) -> &str {
        ""
    }
    fn display_name(&self) -> &str {
        &self.name
    }
}

impl Named for ServiceCredential {
    fn id(&self) -> &str {
        &self.id
    }
    fn slug(&self) -> &str {
        ""
    }
    fn display_name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unknown_fields_survive_a_round_trip() {
        let raw = json!({"id": "ws-1", "displayName": "Acme", "iconUrl": "https://x"});
        let workspace: Workspace = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(workspace.display_name, "Acme");
        assert_eq!(workspace.extra["iconUrl"], "https://x");
        let back = serde_json::to_value(&workspace).unwrap();
        assert_eq!(back["iconUrl"], "https://x");
        assert_eq!(back["id"], "ws-1");
    }

    #[test]
    fn only_the_id_is_required() {
        assert!(serde_json::from_value::<Project>(json!({"slug": "x"})).is_err());
        let project: Project = serde_json::from_value(json!({"id": "p"})).unwrap();
        assert_eq!(project.label(), "p");
    }

    #[test]
    fn cluster_access_accepts_proto_and_short_forms() {
        for (raw, expected) in [
            ("CLUSTER_ACCESS_DEDICATED", ClusterAccess::Dedicated),
            ("dedicated", ClusterAccess::Dedicated),
            ("shared", ClusterAccess::Shared),
            ("CLUSTER_ACCESS_SHARED", ClusterAccess::Shared),
            ("something-new", ClusterAccess::Unspecified),
        ] {
            let cluster: Cluster =
                serde_json::from_value(json!({"id": "c", "access": raw})).unwrap();
            assert_eq!(cluster.access, expected, "{raw}");
        }
        let cluster: Cluster = serde_json::from_value(json!({"id": "c"})).unwrap();
        assert_eq!(cluster.access, ClusterAccess::Unspecified);
    }

    #[test]
    fn labels_prefer_names_over_slugs_over_ids() {
        let tenant: Tenant =
            serde_json::from_value(json!({"id": "t", "slug": "s", "displayName": "D"})).unwrap();
        assert_eq!(tenant.label(), "D");
        let tenant: Tenant = serde_json::from_value(
            json!({"id": "t", "slug": "s", "displayName": "D", "name": "N"}),
        )
        .unwrap();
        assert_eq!(tenant.label(), "N");
        let tenant: Tenant = serde_json::from_value(json!({"id": "t", "slug": "s"})).unwrap();
        assert_eq!(tenant.label(), "s");
    }

    #[test]
    fn databases_serialize_with_their_kind_and_expose_references() {
        let tenant: Tenant = serde_json::from_value(
            json!({"id": "t-1", "projectId": "p", "status": "RESOURCE_STATUS_ACTIVE"}),
        )
        .unwrap();
        let database = Database::Tenant(tenant);
        assert_eq!(
            database.reference(),
            DatabaseReference::Tenant("t-1".into())
        );
        assert_eq!(database.kind(), "tenant");
        assert_eq!(database.project_id(), "p");
        assert_eq!(status_label(database.status()), "active");
        let value = serde_json::to_value(&database).unwrap();
        assert_eq!(value["kind"], "tenant");
        assert_eq!(value["id"], "t-1");

        let cluster: Cluster =
            serde_json::from_value(json!({"id": "c-1", "workspaceId": "w"})).unwrap();
        let database = Database::Dedicated(cluster);
        assert_eq!(
            database.reference(),
            DatabaseReference::Cluster("c-1".into())
        );
        assert_eq!(database.workspace_id(), "w");
        assert_eq!(
            serde_json::to_value(&database).unwrap()["kind"],
            "dedicated"
        );
    }

    #[test]
    fn key_access_labels_cover_enum_raw_and_permission_forms() {
        let key = |value: Value| serde_json::from_value::<DatabaseKey>(value).unwrap();
        assert_eq!(
            key(json!({"id": "k", "access": "DATABASE_KEY_ACCESS_READ_WRITE"})).access_label(),
            "read-write"
        );
        assert_eq!(
            key(json!({"id": "k", "access": "custom"})).access_label(),
            "custom"
        );
        assert_eq!(
            key(json!({"id": "k", "permissions": ["read", "write"]})).access_label(),
            "read,write"
        );
    }
}
