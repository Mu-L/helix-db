use crate::commands::auth::require_auth;
use crate::config::InstanceInfo;
use crate::enterprise_cloud::cloud_base_url;
use crate::local_runtime::LocalRuntime;
use crate::project::ProjectContext;
use chrono::{DateTime, Duration, Utc};
use eyre::{Result, eyre};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct LogsRangeResponse {
    logs: Vec<LogEntry>,
}

#[derive(Debug, Deserialize)]
struct LogEntry {
    message: String,
}

pub async fn run(
    instance: Option<String>,
    follow: bool,
    range: bool,
    start: Option<String>,
    end: Option<String>,
) -> Result<()> {
    let project = ProjectContext::find_and_load(None)?;
    let instance = instance.unwrap_or_else(|| "dev".to_string());
    match project.config.get_instance(&instance)? {
        InstanceInfo::Local(_) => {
            LocalRuntime::new(&project).logs(&instance, follow)?;
        }
        InstanceInfo::Enterprise(config) => {
            if follow {
                return Err(eyre!(
                    "live Enterprise logs are not supported yet; use --range instead"
                ));
            }
            let credentials = require_auth().await?;
            let (start, end) = parse_range(range, start, end)?;
            let logs =
                query_enterprise_logs(&config.cluster_id, &credentials.helix_admin_key, start, end)
                    .await?;
            for line in logs {
                println!("{line}");
            }
        }
    }
    Ok(())
}

fn parse_range(
    range: bool,
    start: Option<String>,
    end: Option<String>,
) -> Result<(DateTime<Utc>, DateTime<Utc>)> {
    let end = match end {
        Some(end) => DateTime::parse_from_rfc3339(&end)?.with_timezone(&Utc),
        None => Utc::now(),
    };
    let start = match start {
        Some(start) => DateTime::parse_from_rfc3339(&start)?.with_timezone(&Utc),
        None if range => end - Duration::hours(1),
        None => end - Duration::hours(1),
    };
    Ok((start, end))
}

async fn query_enterprise_logs(
    cluster_id: &str,
    api_key: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<String>> {
    let url = format!(
        "{}/api/cli/enterprise-clusters/{}/logs/range?start_time={}&end_time={}",
        cloud_base_url(),
        cluster_id,
        start.timestamp(),
        end.timestamp()
    );
    let response = reqwest::Client::new()
        .get(url)
        .header("x-api-key", api_key)
        .send()
        .await?;
    if !response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(eyre!("Failed to fetch Enterprise logs: {body}"));
    }
    let payload: LogsRangeResponse = response.json().await?;
    Ok(payload.logs.into_iter().map(|log| log.message).collect())
}
