use crate::cloud::model::{status_label, Database, DatabaseKey, Named, Tenant};
use crate::cloud::resolve::{Kind, Scope};
use crate::config::DatabaseReference;
use crate::errors::CliError;
use crate::output::{self, table};
use crate::{prompts, DatabaseAction, DatabaseKeyAction, ScopeArgs};
use eyre::{eyre, Result};
use serde_json::{json, Value};

pub async fn run(action: Option<DatabaseAction>) -> Result<()> {
    let scope = Scope::load().await?;
    match action.unwrap_or(DatabaseAction::List {
        scope: ScopeArgs::default(),
    }) {
        DatabaseAction::List { scope: args } => {
            let project = scope.project(&args).await?;
            let databases = scope.databases_in(&project).await?;
            let linked = &scope.link().databases;
            output::emit(&databases, |databases| {
                if databases.is_empty() {
                    output::info(&format!("No databases in {}", project.label()));
                    output::remark("Create one with `helix database create <name>`");
                    return Ok(());
                }
                let mut rows = table::Table::new(["NAME", "KIND", "STATUS", "REF"]);
                for database in databases {
                    let reference = database.reference();
                    rows.row([
                        format!(
                            "{}{}",
                            super::linked_marker(linked.contains(&reference)),
                            database.label()
                        ),
                        database.kind().to_owned(),
                        status_label(database.status()),
                        reference.to_string(),
                    ]);
                }
                rows.print();
                Ok(())
            })
        }
        DatabaseAction::Get {
            database,
            scope: args,
        } => {
            let database = scope.database(database.as_deref(), &args).await?;
            output::emit(&database, |database| {
                print!("{}", details(database));
                Ok(())
            })
        }
        DatabaseAction::Create {
            name,
            slug,
            cluster,
            plan,
            scope: args,
        } => {
            // Settle the placement before any request, so a bad combination
            // fails fast; the plan prompt itself waits until the project is known.
            let placement = match (cluster, plan) {
                (Some(_), Some(_)) => {
                    return Err(
                        CliError::new("--plan only applies to shared tenant databases")
                            .with_hint("omit --plan when creating on a dedicated --cluster")
                            .into(),
                    );
                }
                (Some(cluster), None) => Placement::Dedicated { cluster },
                (None, Some(plan)) if !plan.trim().is_empty() => {
                    Placement::Shared { plan: Some(plan) }
                }
                (None, _) if scope.is_interactive() => Placement::Shared { plan: None },
                (None, _) => {
                    return Err(
                        CliError::new("--plan is required for a shared tenant database")
                            .with_hint(
                                "pass --plan <code>, or --cluster <cluster> to use a dedicated cluster",
                            )
                            .into(),
                    );
                }
            };
            let project = scope.project(&args).await?;
            let (cluster_id, plan) = match placement {
                Placement::Dedicated { cluster } => {
                    let project_scope = ScopeArgs {
                        workspace: None,
                        project: Some(project.id.clone()),
                    };
                    let cluster = scope.cluster(Some(&cluster), &project_scope).await?;
                    (cluster.id, String::new())
                }
                Placement::Shared { plan: Some(plan) } => (String::new(), plan),
                Placement::Shared { plan: None } => {
                    (String::new(), prompts::input_required("Plan code")?)
                }
            };
            let slug = match slug {
                Some(slug) => slug,
                None => super::slugify(&name)?,
            };
            let response = scope
                .client()
                .post(
                    "/v1/tenants",
                    json!({
                        "projectId": project.id,
                        "clusterId": cluster_id,
                        "name": name,
                        "slug": slug,
                        "planCode": plan,
                    }),
                    "create tenant database",
                )
                .await?;
            let token = one_time_token(&response, "database")?;
            let tenant: Tenant = serde_json::from_value(
                response
                    .get("tenant")
                    .cloned()
                    .ok_or_else(|| eyre!("database response omitted the tenant"))?,
            )?;
            output::success(&format!(
                "Created database {} (tenant:{}) in {}",
                tenant.label(),
                tenant.id,
                project.label()
            ));
            output::emit(&response, |_| {
                output::warning(
                    "This read-write application key is shown once. The CLI did not store it.",
                );
                println!("{token}");
                Ok(())
            })
        }
        DatabaseAction::Delete {
            database,
            yes,
            scope: args,
        } => {
            let typed = database
                .as_deref()
                .and_then(|database| database.parse::<DatabaseReference>().ok());
            if let Some(reference @ DatabaseReference::Cluster(_)) = typed {
                return Err(dedicated_delete_error(&reference));
            }
            super::ensure_confirmable(yes)?;
            let database = scope.database(database.as_deref(), &args).await?;
            let Database::Tenant(tenant) = &database else {
                return Err(dedicated_delete_error(&database.reference()));
            };
            let question = format!(
                "Delete database {} (tenant:{})? Its data cannot be recovered.",
                tenant.label(),
                tenant.id
            );
            if !super::confirm(yes, &question)? {
                output::info("Cancelled");
                return Ok(());
            }
            scope
                .client()
                .delete(
                    &format!("/v1/tenants/{}", tenant.id),
                    "delete tenant database",
                )
                .await?;
            output::success(&format!("Deleted database {}", tenant.label()));
            output::emit(
                &json!({"deleted": {"kind": "tenant", "id": tenant.id}}),
                |_| Ok(()),
            )
        }
        DatabaseAction::Indexes {
            database,
            scope: args,
        } => {
            let database = scope.database(database.as_deref(), &args).await?;
            let indexes = scope
                .client()
                .get(
                    &format!("{}/indexes", path(&database.reference())),
                    "list database indexes",
                )
                .await?;
            output::emit(&indexes, |indexes| {
                println!(
                    "{}",
                    output::json::pretty(indexes, console::colors_enabled())
                );
                Ok(())
            })
        }
        DatabaseAction::Key { action } => run_key(&scope, action).await,
    }
}

/// Where a new tenant database lives.
enum Placement {
    /// On a dedicated cluster, named by ID, slug, or name.
    Dedicated { cluster: String },
    /// On shared infrastructure under a plan; `None` prompts for it.
    Shared { plan: Option<String> },
}

async fn run_key(scope: &Scope, action: DatabaseKeyAction) -> Result<()> {
    match action {
        DatabaseKeyAction::Create {
            access,
            name,
            database,
            scope: args,
        } => {
            let database = scope.database(database.as_deref(), &args).await?;
            let response = scope
                .client()
                .post(
                    &format!("{}/keys", path(&database.reference())),
                    json!({
                        "name": name.unwrap_or_default(),
                        "access": access.protobuf_name(),
                    }),
                    "create database key",
                )
                .await?;
            let token = one_time_token(&response, "key")?;
            output::success(&format!("Created a key for {}", database.label()));
            output::emit(&response, |_| {
                output::warning("This application key is shown once. The CLI did not store it.");
                println!("{token}");
                Ok(())
            })
        }
        DatabaseKeyAction::List {
            database,
            scope: args,
        } => {
            let database = scope.database(database.as_deref(), &args).await?;
            let keys = list_keys(scope, &database).await?;
            output::emit(&keys, |keys| {
                if keys.is_empty() {
                    output::info(&format!("{} has no application keys", database.label()));
                    return Ok(());
                }
                let mut rows = table::Table::new(["NAME", "ACCESS", "CREATED", "ID"]);
                for key in keys {
                    rows.row([
                        key.label().to_owned(),
                        key.access_label(),
                        key.created_at.clone(),
                        key.id.clone(),
                    ]);
                }
                rows.print();
                Ok(())
            })
        }
        DatabaseKeyAction::Revoke {
            key,
            database,
            yes,
            scope: args,
        } => {
            super::ensure_confirmable(yes)?;
            let database = scope.database(database.as_deref(), &args).await?;
            let key = scope.pick(Kind::Key, &key, list_keys(scope, &database).await?)?;
            let question = format!(
                "Revoke key {} ({}) on {}? Apps using it lose access immediately.",
                key.label(),
                key.id,
                database.label()
            );
            if !super::confirm(yes, &question)? {
                output::info("Cancelled");
                return Ok(());
            }
            scope
                .client()
                .delete(
                    &format!("{}/keys/{}", path(&database.reference()), key.id),
                    "revoke database key",
                )
                .await?;
            output::success(&format!("Revoked key {}", key.label()));
            output::emit(&json!({"revoked": {"kind": "key", "id": key.id}}), |_| {
                Ok(())
            })
        }
    }
}

async fn list_keys(scope: &Scope, database: &Database) -> Result<Vec<DatabaseKey>> {
    scope
        .client()
        .list(
            &format!("{}/keys", path(&database.reference())),
            &[],
            "keys",
            "list database keys",
        )
        .await
}

/// The WFE path of a database resource.
fn path(reference: &DatabaseReference) -> String {
    match reference {
        DatabaseReference::Cluster(id) => format!("/v1/clusters/{id}"),
        DatabaseReference::Tenant(id) => format!("/v1/tenants/{id}"),
    }
}

/// The one-time token in a create response. It is never stored.
fn one_time_token(response: &Value, what: &str) -> Result<String> {
    response
        .get("token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| eyre!("the {what} response omitted its one-time token"))
}

fn dedicated_delete_error(reference: &DatabaseReference) -> eyre::Report {
    CliError::new(format!(
        "{reference} is a dedicated cluster; the CLI only deletes tenant databases"
    ))
    .with_hint("manage dedicated clusters in the Helix dashboard")
    .into()
}

fn details(database: &Database) -> String {
    let cluster = match database {
        Database::Tenant(tenant) => tenant.cluster_id.clone(),
        Database::Dedicated(_) => String::new(),
    };
    table::key_values(&[
        ("Name", database.label().to_owned()),
        ("Ref", database.reference().to_string()),
        ("Kind", database.kind().to_owned()),
        ("Status", status_label(database.status())),
        ("Cluster", cluster),
        ("Project", database.project_id().to_owned()),
        ("Workspace", database.workspace_id().to_owned()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_follow_the_reference_kind() {
        assert_eq!(
            path(&DatabaseReference::Tenant("t".into())),
            "/v1/tenants/t"
        );
        assert_eq!(
            path(&DatabaseReference::Cluster("c".into())),
            "/v1/clusters/c"
        );
    }

    #[test]
    fn tokens_must_be_present_and_non_empty() {
        assert_eq!(one_time_token(&json!({"token": "s"}), "key").unwrap(), "s");
        assert!(one_time_token(&json!({"token": ""}), "key").is_err());
        assert!(one_time_token(&json!({}), "key").is_err());
    }
}
