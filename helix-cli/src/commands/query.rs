use crate::config::InstanceInfo;
use crate::project::ProjectContext;
use eyre::{Result, eyre};
use reqwest::header::{CONTENT_TYPE, HeaderName, HeaderValue};
use serde_json::Value;

pub async fn run(
    instance: Option<String>,
    file: String,
    warm: bool,
    host: Option<String>,
    port: Option<u16>,
    compact: bool,
) -> Result<()> {
    let project = ProjectContext::find_and_load(None)?;
    let instance = instance.unwrap_or_else(|| "dev".to_string());
    let request_text = std::fs::read_to_string(&file)
        .map_err(|e| eyre!("Failed to read query request file '{file}': {e}"))?;
    let request_json: Value = serde_json::from_str(&request_text)
        .map_err(|e| eyre!("Failed to parse query request file '{file}': {e}"))?;

    validate_dynamic_request(&request_json, warm)?;
    let client = reqwest::Client::new();
    let mut request = match project.config.get_instance(&instance)? {
        InstanceInfo::Local(config) => {
            let host = host.unwrap_or_else(|| "localhost".to_string());
            let port = port.unwrap_or(config.port);
            client.post(format!("http://{host}:{port}/v1/query"))
        }
        InstanceInfo::Enterprise(config) => {
            let gateway_url = config.gateway_url.as_deref().ok_or_else(|| {
                eyre!(
                    "Enterprise gateway URL is not configured for '{instance}'. Run 'helix sync {instance}' or set gateway_url in helix.toml."
                )
            })?;
            let auth_value = std::env::var(&config.query_auth_env).map_err(|_| {
                eyre!(
                    "Environment variable {} is required for Enterprise query auth",
                    config.query_auth_env
                )
            })?;
            let header_name = HeaderName::from_bytes(config.query_auth_header.as_bytes())?;
            client
                .post(format!("{}/v1/query", gateway_url.trim_end_matches('/')))
                .header(header_name, HeaderValue::from_str(&auth_value)?)
        }
    };

    request = request.header(CONTENT_TYPE, "application/json");
    if warm {
        request = request.header("X-Helix-Warm", "true");
    }

    let response = request.json(&request_json).send().await?;
    let status = response.status();
    if status == reqwest::StatusCode::NO_CONTENT {
        return Ok(());
    }
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(eyre!("Query failed with HTTP {status}: {body}"));
    }

    if body.trim().is_empty() {
        return Ok(());
    }
    let value: Value = serde_json::from_str(&body).unwrap_or(Value::String(body));
    if compact {
        println!("{}", serde_json::to_string(&value)?);
    } else {
        println!("{}", serde_json::to_string_pretty(&value)?);
    }
    Ok(())
}

fn validate_dynamic_request(request: &Value, warm: bool) -> Result<()> {
    let request_type = request
        .get("request_type")
        .and_then(Value::as_str)
        .ok_or_else(|| eyre!("dynamic query request must include request_type"))?;
    if request_type != "read" && request_type != "write" {
        return Err(eyre!("request_type must be lowercase 'read' or 'write'"));
    }
    if warm && request_type != "read" {
        return Err(eyre!("--warm is only valid for read requests"));
    }
    if request.get("query").is_none() {
        return Err(eyre!("dynamic query request must include query"));
    }
    Ok(())
}
