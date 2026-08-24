mod support;

use assert_cmd::assert::Assert;
use serde_json::Value;
use support::CliFixture;
use wiremock::matchers::{body_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn stdout(assert: Assert) -> String {
    String::from_utf8(assert.get_output().stdout.clone()).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn database_keys_are_explicit_and_service_credentials_are_never_stored() {
    let server = MockServer::start().await;
    let fixture = CliFixture::new().with_http_base(server.uri());
    fixture.write_credentials("owner@example.com", "session-access");

    Mock::given(method("POST"))
        .and(path("/v1/tenants"))
        .and(header("authorization", "Bearer session-access"))
        .and(body_json(serde_json::json!({
            "projectId":"project-1", "clusterId":"", "name":"App", "slug":"app",
            "planCode":"starter"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "tenant":{"id":"tenant-1","projectId":"project-1","name":"App"}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let created = stdout(
        fixture
            .command()
            .args([
                "database",
                "create",
                "--project",
                "project-1",
                "--name",
                "App",
                "--slug",
                "app",
                "--plan",
                "starter",
                "--format",
                "json",
            ])
            .assert()
            .success(),
    );
    let created: Value = serde_json::from_str(&created).unwrap();
    assert_eq!(created["tenant"]["id"], "tenant-1");
    assert!(created.get("token").is_none());

    Mock::given(method("POST"))
        .and(path("/v1/tenants/tenant-1/keys"))
        .and(body_json(serde_json::json!({
            "name":"application", "access":"DATABASE_KEY_ACCESS_READ_WRITE"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "token":"database-secret", "key":{"id":"key-1"}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let key = stdout(
        fixture
            .command()
            .args([
                "database",
                "key",
                "create",
                "tenant:tenant-1",
                "--name",
                "application",
                "--access",
                "read-write",
            ])
            .assert()
            .success(),
    );
    assert_eq!(key.trim(), "database-secret");

    Mock::given(method("POST"))
        .and(path("/v1/workspaces/ws-1/service-credentials"))
        .and(body_json(serde_json::json!({
            "workspaceId":"ws-1",
            "name":"automation",
            "grants":[{"projectId":"project-1","permissions":["SERVICE_CREDENTIAL_PERMISSION_DATABASE_QUERY_READ"]}],
            "expiresAt":null
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "credential":{"id":"svc-1"}, "token":"service-secret"
        })))
        .expect(1)
        .mount(&server)
        .await;
    let credential = stdout(
        fixture
            .command()
            .args([
                "service-credential",
                "create",
                "--workspace",
                "ws-1",
                "--name",
                "automation",
                "--grant",
                "project-1=query-read",
            ])
            .assert()
            .success(),
    );
    assert_eq!(credential.trim(), "service-secret");
    let stored = std::fs::read_to_string(fixture.helix_home().join("credentials")).unwrap();
    assert!(!stored.contains("database-secret"));
    assert!(!stored.contains("service-secret"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn discovery_uses_explicit_workspace_and_project_filters() {
    let server = MockServer::start().await;
    let fixture = CliFixture::new().with_http_base(server.uri());
    fixture.write_credentials("owner@example.com", "session-access");
    Mock::given(method("GET"))
        .and(path("/v1/projects"))
        .and(query_param("workspaceId", "ws-1"))
        .and(header("authorization", "Bearer session-access"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "projects":[{"id":"project-1","displayName":"Graph"}]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let projects = stdout(
        fixture
            .command()
            .args([
                "project",
                "list",
                "--workspace-id",
                "ws-1",
                "--format",
                "json",
            ])
            .assert()
            .success(),
    );
    assert_eq!(
        serde_json::from_str::<Value>(&projects).unwrap()["projects"][0]["id"],
        "project-1"
    );
}
