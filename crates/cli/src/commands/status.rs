use crate::cloud::model::{status_label, Cluster, Named as _, Tenant};
use crate::cloud::CloudClient;
use crate::commands::auth::require_auth;
use crate::config::{DatabaseReference, InstanceInfo};
use crate::errors::CliError;
use crate::local_runtime::LocalRuntime;
use crate::output::{self, table};
use crate::project::ProjectContext;
use console::style;
use eyre::Result;
use serde::Serialize;
use serde_json::Value;
use std::path::Path;

#[derive(Serialize)]
struct Report<'a> {
    project: &'a str,
    root: &'a Path,
    instances: Vec<InstanceStatus>,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum InstanceStatus {
    Local {
        name: String,
        state: String,
        url: String,
        storage: String,
    },
    Cloud {
        name: String,
        state: String,
        database: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        /// Why the state could not be read; one unreachable Cloud database
        /// never hides the status of the others.
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

/// Every instance's state (or just `instance`'s): local containers from the
/// runtime, Cloud databases from the API.
pub async fn run(instance: Option<String>) -> Result<()> {
    let project = ProjectContext::find_and_load(None)?;
    let names = match instance {
        Some(name) => {
            project.config.get_instance(&name)?;
            vec![name]
        }
        None => project
            .config
            .list_instances()
            .into_iter()
            .cloned()
            .collect(),
    };
    let runtime = LocalRuntime::new(&project);
    let needs_cloud = names
        .iter()
        .any(|name| project.config.enterprise.contains_key(name));
    let client = if needs_cloud {
        Some(
            require_auth()
                .await
                .map_err(|error| CliError::from_report(&error).message),
        )
    } else {
        None
    };

    let mut instances = Vec::with_capacity(names.len());
    for name in names {
        instances.push(match project.config.get_instance(&name)? {
            InstanceInfo::Local(config) => InstanceStatus::Local {
                state: runtime
                    .status(&name)?
                    .map_or_else(|| "not created".to_owned(), |status| status.status),
                url: format!("http://localhost:{}", config.port),
                storage: config.storage.as_str().to_owned(),
                name,
            },
            InstanceInfo::Enterprise(config) => {
                let client = client
                    .as_ref()
                    .expect("a client is loaded for Cloud instances");
                match client {
                    Ok(client) => cloud_status(client, name, &config.database).await,
                    Err(message) => InstanceStatus::Cloud {
                        name,
                        state: "unknown".to_owned(),
                        database: config.database.to_string(),
                        label: None,
                        error: Some(message.clone()),
                    },
                }
            }
        });
    }

    let report = Report {
        project: &project.config.project.name,
        root: &project.root,
        instances,
    };
    output::emit(&report, |report| {
        print!(
            "{}",
            table::key_values(&[
                ("Project", report.project.to_owned()),
                ("Root", report.root.display().to_string()),
            ])
        );
        println!();
        let mut rows = table::Table::new(["INSTANCE", "KIND", "STATUS", "ENDPOINT"]);
        for instance in &report.instances {
            rows.row(match instance {
                InstanceStatus::Local {
                    name,
                    state,
                    url,
                    storage,
                } => [
                    name.clone(),
                    "local".to_owned(),
                    styled_state(state),
                    format!("{url} {}", style(format!("({storage})")).dim()),
                ],
                InstanceStatus::Cloud {
                    name,
                    state,
                    database,
                    label,
                    ..
                } => [
                    name.clone(),
                    "cloud".to_owned(),
                    styled_state(state),
                    match label {
                        Some(label) => format!("{label} {}", style(format!("({database})")).dim()),
                        None => database.clone(),
                    },
                ],
            });
        }
        rows.print();
        for instance in &report.instances {
            let InstanceStatus::Cloud {
                name,
                error: Some(error),
                ..
            } = instance
            else {
                continue;
            };
            output::warning(&format!("{name}: {error}"));
        }
        Ok(())
    })
}

async fn cloud_status(
    client: &CloudClient,
    name: String,
    database: &DatabaseReference,
) -> InstanceStatus {
    let state = match database {
        DatabaseReference::Tenant(id) => client
            .fetch::<Tenant>(&format!("/v1/tenants/{id}"), "get Cloud tenant status")
            .await
            .map(|tenant| (tenant.label().to_owned(), status_label(&tenant.status))),
        DatabaseReference::Cluster(id) => {
            let cluster = client
                .fetch::<Cluster>(&format!("/v1/clusters/{id}"), "get Cloud cluster status")
                .await;
            match cluster {
                Ok(cluster) => client
                    .get(
                        &format!("/v1/clusters/{id}/topology"),
                        "get Cloud cluster topology",
                    )
                    .await
                    .map(|topology| {
                        let state = ["phase", "status", "state"]
                            .into_iter()
                            .find_map(|field| topology.get(field).and_then(Value::as_str))
                            .unwrap_or(&cluster.status);
                        (cluster.label().to_owned(), status_label(state))
                    }),
                Err(error) => Err(error),
            }
        }
    };
    match state {
        Ok((label, state)) => InstanceStatus::Cloud {
            name,
            state,
            database: database.to_string(),
            label: Some(label),
            error: None,
        },
        Err(error) => InstanceStatus::Cloud {
            name,
            state: "unreachable".to_owned(),
            database: database.to_string(),
            label: None,
            error: Some(CliError::from_report(&error).to_string()),
        },
    }
}

/// Healthy states in green, failures in red, everything else dim.
fn styled_state(state: &str) -> String {
    let lower = state.to_lowercase();
    let styled =
        if lower.starts_with("up") || matches!(lower.as_str(), "running" | "active" | "ready") {
            style(state).green()
        } else if matches!(lower.as_str(), "unreachable" | "failed" | "unknown")
            || lower.starts_with("exited")
        {
            style(state).red()
        } else {
            style(state).dim()
        };
    styled.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_styling_never_changes_the_text() {
        for state in [
            "Up 3 minutes",
            "active",
            "exited (1)",
            "not created",
            "unreachable",
        ] {
            assert_eq!(console::strip_ansi_codes(&styled_state(state)), state);
        }
    }

    #[test]
    fn statuses_serialize_with_their_kind() {
        let local = serde_json::to_value(InstanceStatus::Local {
            name: "dev".into(),
            state: "not created".into(),
            url: "http://localhost:6969".into(),
            storage: "memory".into(),
        })
        .unwrap();
        assert_eq!(local["kind"], "local");
        let cloud = serde_json::to_value(InstanceStatus::Cloud {
            name: "prod".into(),
            state: "unreachable".into(),
            database: "tenant:t".into(),
            label: None,
            error: Some("HTTP 503".into()),
        })
        .unwrap();
        assert_eq!(cloud["kind"], "cloud");
        assert_eq!(cloud["error"], "HTTP 503");
        assert!(cloud.get("label").is_none());
    }
}
