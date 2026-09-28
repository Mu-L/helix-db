//! Resolving Cloud resources from optional arguments.
//!
//! Every command resolves workspaces, projects, clusters, and databases the
//! same way, so no command ever requires a raw ID:
//!
//! 1. an explicit argument, matched by ID, then slug, then display name;
//! 2. the project and databases linked in helix.toml;
//! 3. the only candidate;
//! 4. an interactive picker (a TTY, and never under `--json`);
//! 5. otherwise an error listing the candidates and how to pass one.

use super::model::{Cluster, ClusterAccess, Database, Named, Project, Tenant, Workspace};
use super::CloudClient;
use crate::config::{DatabaseReference, HelixConfig};
use crate::errors::{Candidate, CliError, ProjectError};
use crate::project::ProjectContext;
use crate::{output, prompts, ScopeArgs};
use eyre::Result;
use serde::de::DeserializeOwned;
use std::collections::BTreeSet;

/// The kinds of resource the resolver can pick, for messages and hints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Workspace,
    Project,
    Cluster,
    Database,
    Key,
    ServiceCredential,
}

impl Kind {
    fn noun(self) -> &'static str {
        match self {
            Self::Workspace => "workspace",
            Self::Project => "project",
            Self::Cluster => "cluster",
            Self::Database => "database",
            Self::Key => "key",
            Self::ServiceCredential => "service credential",
        }
    }

    fn plural(self) -> &'static str {
        match self {
            Self::Workspace => "workspaces",
            Self::Project => "projects",
            Self::Cluster => "clusters",
            Self::Database => "databases",
            Self::Key => "keys",
            Self::ServiceCredential => "service credentials",
        }
    }

    /// How to name one of these on the command line.
    fn how_to_pass(self) -> &'static str {
        match self {
            Self::Workspace => "pass --workspace <id|slug|name>",
            Self::Project => {
                "pass --project <id|slug|name>, or link one here with `helix project link`"
            }
            Self::Cluster => "pass the cluster by ID, slug, or name",
            Self::Database => {
                "pass the database by ID, slug, or name, or as tenant:<id> / cluster:<id>"
            }
            Self::Key => "pass the key by ID or name",
            Self::ServiceCredential => "pass the credential by ID or name",
        }
    }

    /// What to do when there are none at all.
    fn when_empty(self) -> &'static str {
        match self {
            Self::Workspace => {
                "create a workspace in the Helix dashboard, or ask to be invited to one"
            }
            Self::Project => "create one with `helix project create <name>`",
            Self::Cluster => "this project has no clusters",
            Self::Database => "create one with `helix database create <name>`",
            Self::Key => "create one with `helix database key create --access read-write`",
            Self::ServiceCredential => {
                "create one with `helix service-credential create --name <name> --grant ...`"
            }
        }
    }
}

/// The outcome of matching a query against candidates.
#[derive(Debug, PartialEq)]
pub enum Resolution<T> {
    Found(T),
    /// More than one candidate matched at the first tier that matched at all.
    Ambiguous(Vec<T>),
    /// Nothing matched; carries every candidate so the error can list them.
    NotFound(Vec<T>),
}

/// Match `query` against candidates by exact ID (or typed reference), then
/// exact slug, then case-insensitive display name. The first tier with any
/// match decides, so an ID always wins over a name that happens to equal it.
///
/// ```
/// use helix_cli::cloud::model::Workspace;
/// use helix_cli::cloud::resolve::{match_query, Resolution};
///
/// let workspace = |id: &str, slug: &str, name: &str| -> Workspace {
///     serde_json::from_value(serde_json::json!({"id": id, "slug": slug, "displayName": name}))
///         .unwrap()
/// };
/// let all = vec![workspace("ws-1", "acme", "Acme"), workspace("ws-2", "beta", "Beta")];
/// assert!(matches!(match_query(all.clone(), "ws-2"), Resolution::Found(w) if w.id == "ws-2"));
/// assert!(matches!(match_query(all.clone(), "acme"), Resolution::Found(w) if w.id == "ws-1"));
/// assert!(matches!(match_query(all.clone(), "BETA"), Resolution::Found(w) if w.id == "ws-2"));
/// assert!(matches!(match_query(all, "gamma"), Resolution::NotFound(all) if all.len() == 2));
/// ```
pub fn match_query<T: Named>(candidates: Vec<T>, query: &str) -> Resolution<T> {
    let by_id = |candidate: &T| candidate.id() == query || candidate.argument() == query;
    let by_slug = |candidate: &T| !candidate.slug().is_empty() && candidate.slug() == query;
    let lowered = query.to_lowercase();
    let by_name = |candidate: &T| {
        !candidate.display_name().is_empty() && candidate.display_name().to_lowercase() == lowered
    };
    let tiers: [&dyn Fn(&T) -> bool; 3] = [&by_id, &by_slug, &by_name];
    let Some(tier) = tiers.into_iter().find(|tier| candidates.iter().any(tier)) else {
        return Resolution::NotFound(candidates);
    };
    let mut matches: Vec<T> = candidates
        .into_iter()
        .filter(|candidate| tier(candidate))
        .collect();
    match matches.len() {
        1 => Resolution::Found(matches.remove(0)),
        _ => Resolution::Ambiguous(matches),
    }
}

impl<T: Named> From<&T> for Candidate {
    fn from(resource: &T) -> Self {
        Candidate {
            id: resource.argument(),
            name: resource.label().to_owned(),
        }
    }
}

/// What this directory's helix.toml links to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Link {
    pub project: Option<ProjectLink>,
    /// The databases of every `[enterprise.*]` instance, sorted and deduplicated.
    pub databases: Vec<DatabaseReference>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectLink {
    pub project_id: String,
    pub workspace_id: Option<String>,
}

impl Link {
    /// The link of the helix.toml above the current directory. No helix.toml
    /// means no link; a broken one is an error rather than silently ignored.
    pub fn load() -> Result<Self> {
        match ProjectContext::find_and_load(None) {
            Ok(context) => Ok(Self::from_config(&context.config)),
            Err(ProjectError::ConfigNotFound { .. }) => Ok(Self::default()),
            Err(error) => Err(error.into()),
        }
    }

    /// `[project] id` wins; otherwise the Cloud instances' owner, when they all
    /// agree on one project.
    pub fn from_config(config: &HelixConfig) -> Self {
        let owners: BTreeSet<(String, Option<String>)> = config
            .enterprise
            .values()
            .filter_map(|instance| {
                instance
                    .project_id
                    .clone()
                    .map(|project_id| (project_id, instance.workspace_id.clone()))
            })
            .collect();
        let shared_owner = match owners.into_iter().collect::<Vec<_>>().as_slice() {
            [(project_id, workspace_id)] => Some(ProjectLink {
                project_id: project_id.clone(),
                workspace_id: workspace_id.clone(),
            }),
            _ => None,
        };
        let project = config
            .project
            .id
            .clone()
            .map(|project_id| ProjectLink {
                project_id,
                workspace_id: config.project.workspace_id.clone(),
            })
            .or(shared_owner);
        let databases = config
            .enterprise
            .values()
            .map(|instance| instance.database.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        Self { project, databases }
    }
}

/// Resolves Cloud resources for one command invocation.
pub struct Scope {
    client: CloudClient,
    link: Link,
    interactive: bool,
}

impl Scope {
    /// An authenticated scope for the current directory's helix.toml link.
    pub async fn load() -> Result<Self> {
        Self::new(Link::load()?).await
    }

    /// An authenticated scope with an explicit link.
    pub async fn new(link: Link) -> Result<Self> {
        Ok(Self {
            client: crate::commands::auth::require_auth().await?,
            link,
            interactive: prompts::is_interactive(),
        })
    }

    /// The same scope without the helix.toml link, e.g. to relink a
    /// directory or to initialize a new project inside an existing one.
    pub fn unlinked(self) -> Self {
        Self {
            link: Link::default(),
            ..self
        }
    }

    pub fn client(&self) -> &CloudClient {
        &self.client
    }

    pub fn link(&self) -> &Link {
        &self.link
    }

    pub fn is_interactive(&self) -> bool {
        self.interactive
    }

    // ------------------------------------------------------------------
    // Workspaces
    // ------------------------------------------------------------------

    pub async fn workspaces(&self) -> Result<Vec<Workspace>> {
        self.client
            .list("/v1/workspaces", &[], "workspaces", "list workspaces")
            .await
    }

    pub async fn workspace(&self, arg: Option<&str>) -> Result<Workspace> {
        match (arg, &self.link.project) {
            (Some(query), _) => self.pick(Kind::Workspace, query, self.workspaces().await?),
            (None, Some(link)) => {
                let workspace_id = match &link.workspace_id {
                    Some(workspace_id) => workspace_id.clone(),
                    None => self.linked_project(link).await?.workspace_id,
                };
                self.linked(
                    Kind::Workspace,
                    &format!("/v1/workspaces/{workspace_id}"),
                    &workspace_id,
                )
                .await
            }
            (None, None) => self.choose(Kind::Workspace, self.workspaces().await?),
        }
    }

    // ------------------------------------------------------------------
    // Projects
    // ------------------------------------------------------------------

    pub async fn projects_in(&self, workspace: &Workspace) -> Result<Vec<Project>> {
        let projects: Vec<Project> = self
            .client
            .list(
                "/v1/projects",
                &[("workspace_id", &workspace.id)],
                "projects",
                "list projects",
            )
            .await?;
        Ok(projects
            .into_iter()
            .map(|mut project| {
                if project.workspace_id.is_empty() {
                    project.workspace_id = workspace.id.clone();
                }
                project
            })
            .collect())
    }

    pub async fn project(&self, args: &ScopeArgs) -> Result<Project> {
        match (args.project.as_deref(), &self.link.project, &args.workspace) {
            (Some(query), _, Some(workspace)) => {
                let workspace = self.workspace(Some(workspace)).await?;
                self.pick(Kind::Project, query, self.projects_in(&workspace).await?)
            }
            (Some(query), _, None) => {
                // An ID resolves directly; a slug or name is searched for in
                // every accessible workspace, so it works outside a project.
                let path = format!("/v1/projects/{}", urlencoding::encode(query));
                match self.client.fetch::<Project>(&path, "get project").await {
                    Ok(project) => Ok(project),
                    Err(_) => self.pick(Kind::Project, query, self.all_projects().await?),
                }
            }
            (None, Some(link), None) => self.linked_project(link).await,
            (None, _, workspace) => {
                let workspace = self.workspace(workspace.as_deref()).await?;
                self.choose(Kind::Project, self.projects_in(&workspace).await?)
            }
        }
    }

    async fn linked_project(&self, link: &ProjectLink) -> Result<Project> {
        self.linked(
            Kind::Project,
            &format!("/v1/projects/{}", link.project_id),
            &link.project_id,
        )
        .await
    }

    /// Projects across every accessible workspace, fetched concurrently.
    async fn all_projects(&self) -> Result<Vec<Project>> {
        let mut requests = tokio::task::JoinSet::new();
        for workspace in self.workspaces().await? {
            let client = self.client.clone();
            requests.spawn(async move {
                let projects: Vec<Project> = client
                    .list(
                        "/v1/projects",
                        &[("workspace_id", &workspace.id)],
                        "projects",
                        "list projects",
                    )
                    .await?;
                eyre::Ok(
                    projects
                        .into_iter()
                        .map(|mut project| {
                            project.workspace_id = workspace.id.clone();
                            project
                        })
                        .collect::<Vec<_>>(),
                )
            });
        }
        let mut projects = Vec::new();
        while let Some(joined) = requests.join_next().await {
            projects.extend(joined??);
        }
        // Completion order is arbitrary; keep candidate lists stable.
        projects.sort_by(|a, b| (a.label(), &a.id).cmp(&(b.label(), &b.id)));
        Ok(projects)
    }

    // ------------------------------------------------------------------
    // Clusters
    // ------------------------------------------------------------------

    pub async fn clusters_in_project(&self, project: &Project) -> Result<Vec<Cluster>> {
        self.client
            .list(
                "/v1/clusters",
                &[
                    ("workspace_id", &project.workspace_id),
                    ("project_id", &project.id),
                ],
                "clusters",
                "list project clusters",
            )
            .await
    }

    pub async fn clusters_in_workspace(&self, workspace: &Workspace) -> Result<Vec<Cluster>> {
        self.client
            .list(
                "/v1/clusters",
                &[("workspace_id", &workspace.id)],
                "clusters",
                "list workspace clusters",
            )
            .await
    }

    pub async fn cluster(&self, arg: Option<&str>, args: &ScopeArgs) -> Result<Cluster> {
        let unscoped = args.project.is_none() && args.workspace.is_none();
        let linked: Vec<&str> = self
            .link
            .databases
            .iter()
            .filter_map(|database| match database {
                DatabaseReference::Cluster(id) => Some(id.as_str()),
                DatabaseReference::Tenant(_) => None,
            })
            .collect();
        match (arg, linked.as_slice()) {
            (Some(query), _) => {
                let id = query.strip_prefix("cluster:").unwrap_or(query);
                let path = format!("/v1/clusters/{}", urlencoding::encode(id));
                match self.client.fetch::<Cluster>(&path, "get cluster").await {
                    Ok(cluster) => Ok(cluster),
                    Err(_) => {
                        let project = self.project(args).await?;
                        self.pick(
                            Kind::Cluster,
                            query,
                            self.clusters_in_project(&project).await?,
                        )
                    }
                }
            }
            (None, [only]) if unscoped => {
                self.linked(Kind::Cluster, &format!("/v1/clusters/{only}"), only)
                    .await
            }
            (None, _) => {
                let project = self.project(args).await?;
                self.choose(Kind::Cluster, self.clusters_in_project(&project).await?)
            }
        }
    }

    // ------------------------------------------------------------------
    // Databases
    // ------------------------------------------------------------------

    /// A project's queryable databases: its dedicated clusters and tenants.
    pub async fn databases_in(&self, project: &Project) -> Result<Vec<Database>> {
        let clusters = self.clusters_in_project(project).await?;
        let tenants: Vec<Tenant> = self
            .client
            .list(
                "/v1/tenants",
                &[
                    ("workspace_id", &project.workspace_id),
                    ("project_id", &project.id),
                ],
                "tenants",
                "list project tenants",
            )
            .await?;
        let owned = |project_id: &mut String, workspace_id: &mut String| {
            if project_id.is_empty() {
                project_id.clone_from(&project.id);
            }
            if workspace_id.is_empty() {
                workspace_id.clone_from(&project.workspace_id);
            }
        };
        Ok(clusters
            .into_iter()
            .filter(|cluster| cluster.access == ClusterAccess::Dedicated)
            .map(|mut cluster| {
                owned(&mut cluster.project_id, &mut cluster.workspace_id);
                Database::Dedicated(cluster)
            })
            .chain(tenants.into_iter().map(|mut tenant| {
                owned(&mut tenant.project_id, &mut tenant.workspace_id);
                Database::Tenant(tenant)
            }))
            .collect())
    }

    /// Fetch a database by its typed reference. A shared cluster is not a
    /// database, so it is rejected.
    pub async fn database_by_reference(&self, reference: &DatabaseReference) -> Result<Database> {
        match reference {
            DatabaseReference::Tenant(id) => Ok(Database::Tenant(
                self.client
                    .fetch(&format!("/v1/tenants/{id}"), "get tenant")
                    .await?,
            )),
            DatabaseReference::Cluster(id) => {
                let cluster: Cluster = self
                    .client
                    .fetch(&format!("/v1/clusters/{id}"), "get cluster")
                    .await?;
                if cluster.access != ClusterAccess::Dedicated {
                    return Err(CliError::new(format!(
                        "cluster {id} is shared, so it is not a database"
                    ))
                    .with_hint("pass one of its tenants as tenant:<id> instead")
                    .into());
                }
                Ok(Database::Dedicated(cluster))
            }
        }
    }

    pub async fn database(&self, arg: Option<&str>, args: &ScopeArgs) -> Result<Database> {
        let typed = arg.and_then(|arg| arg.parse::<DatabaseReference>().ok());
        let Some(reference) = typed else {
            let unscoped = args.project.is_none() && args.workspace.is_none();
            return match (arg, self.link.databases.as_slice()) {
                (Some(query), _) => {
                    let project = self.project(args).await?;
                    self.pick(Kind::Database, query, self.databases_in(&project).await?)
                }
                (None, [only]) if unscoped => self
                    .database_by_reference(only)
                    .await
                    .map_err(|error| stale_link(Kind::Database, &only.to_string(), &error)),
                (None, [_, _, ..]) if unscoped => {
                    let mut linked = Vec::with_capacity(self.link.databases.len());
                    for reference in &self.link.databases {
                        linked.push(self.database_by_reference(reference).await.map_err(
                            |error| stale_link(Kind::Database, &reference.to_string(), &error),
                        )?);
                    }
                    self.choose(Kind::Database, linked)
                }
                (None, _) => {
                    let project = self.project(args).await?;
                    self.choose(Kind::Database, self.databases_in(&project).await?)
                }
            };
        };
        self.database_by_reference(&reference).await
    }

    /// The project (and workspace) that owns `database`.
    pub async fn owner(&self, database: &Database) -> Result<ProjectLink> {
        let project_id = database.project_id();
        if project_id.is_empty() {
            return Err(eyre::eyre!(
                "the Cloud response for {} omitted its project",
                database.reference()
            ));
        }
        let workspace_id = match database.workspace_id() {
            "" => {
                self.client
                    .fetch::<Project>(&format!("/v1/projects/{project_id}"), "get project")
                    .await?
                    .workspace_id
            }
            workspace_id => workspace_id.to_owned(),
        };
        Ok(ProjectLink {
            project_id: project_id.to_owned(),
            workspace_id: Some(workspace_id).filter(|workspace_id| !workspace_id.is_empty()),
        })
    }

    // ------------------------------------------------------------------
    // Shared resolution steps
    // ------------------------------------------------------------------

    /// Fetch a resource named by the helix.toml link, explaining a stale link.
    async fn linked<T: DeserializeOwned>(&self, kind: Kind, path: &str, id: &str) -> Result<T> {
        self.client
            .fetch(path, &format!("get linked {}", kind.noun()))
            .await
            .map_err(|error| stale_link(kind, id, &error))
    }

    /// Resolve an explicit argument among `candidates`.
    pub fn pick<T: Named>(&self, kind: Kind, query: &str, candidates: Vec<T>) -> Result<T> {
        match match_query(candidates, query) {
            Resolution::Found(found) => Ok(found),
            Resolution::Ambiguous(matches) if self.interactive => self.prompt(kind, matches),
            Resolution::Ambiguous(matches) => Err(CliError::new(format!(
                "'{query}' matches {} {}",
                matches.len(),
                kind.plural()
            ))
            .with_hint(kind.how_to_pass())
            .with_candidates(matches.iter().map(Candidate::from).collect())
            .into()),
            Resolution::NotFound(all) => Err(CliError::new(format!(
                "no {} matches '{query}'",
                kind.noun()
            ))
            .with_hint(kind.how_to_pass())
            .with_candidates(all.iter().map(Candidate::from).collect())
            .into()),
        }
    }

    /// Choose among `candidates` without an explicit argument: the only one,
    /// else a prompt, else an error listing them.
    pub fn choose<T: Named>(&self, kind: Kind, mut candidates: Vec<T>) -> Result<T> {
        match candidates.len() {
            0 => Err(CliError::new(format!("no {} found", kind.plural()))
                .with_hint(kind.when_empty())
                .into()),
            1 => {
                let only = candidates.remove(0);
                output::remark(&format!("Using {} {}", kind.noun(), only.label()));
                Ok(only)
            }
            _ if self.interactive => self.prompt(kind, candidates),
            _ => Err(CliError::new(format!(
                "{} {} found; choose one",
                candidates.len(),
                kind.plural()
            ))
            .with_hint(kind.how_to_pass())
            .with_candidates(candidates.iter().map(Candidate::from).collect())
            .into()),
        }
    }

    fn prompt<T: Named>(&self, kind: Kind, mut candidates: Vec<T>) -> Result<T> {
        let mut select = cliclack::select(format!("Select a {}", kind.noun()));
        for (index, candidate) in candidates.iter().enumerate() {
            let argument = candidate.argument();
            let hint = [candidate.slug(), argument.as_str()]
                .into_iter()
                .filter(|part| !part.is_empty() && *part != candidate.label())
                .collect::<Vec<_>>()
                .join(" · ");
            select = select.item(index, candidate.label(), hint);
        }
        let index: usize = select.interact()?;
        Ok(candidates.swap_remove(index))
    }
}

fn stale_link(kind: Kind, id: &str, error: &eyre::Report) -> eyre::Report {
    let hint = match kind {
        Kind::Workspace | Kind::Project => "relink this directory with `helix project link`",
        _ => "update the [enterprise.*] entry in helix.toml, or re-add it with `helix add cloud`",
    };
    CliError::new(format!(
        "the {} linked in helix.toml ({id}) could not be loaded",
        kind.noun()
    ))
    .with_caused_by(error.to_string())
    .with_hint(hint)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EnterpriseInstanceConfig;
    use serde_json::json;

    fn project(id: &str, slug: &str, name: &str) -> Project {
        serde_json::from_value(json!({"id": id, "slug": slug, "displayName": name})).unwrap()
    }

    #[test]
    fn ids_win_over_slugs_and_names() {
        let candidates = vec![project("api", "x", "y"), project("p-2", "api", "api")];
        let Resolution::Found(found) = match_query(candidates, "api") else {
            panic!("an exact ID must resolve");
        };
        assert_eq!(found.id, "api");
    }

    #[test]
    fn slugs_win_over_names() {
        let candidates = vec![project("p-1", "x", "web"), project("p-2", "web", "Other")];
        let Resolution::Found(found) = match_query(candidates, "web") else {
            panic!("an exact slug must resolve");
        };
        assert_eq!(found.id, "p-2");
    }

    #[test]
    fn duplicate_names_are_ambiguous_and_empty_fields_never_match() {
        let candidates = vec![
            project("p-1", "", "Web"),
            project("p-2", "", "web"),
            project("p-3", "", ""),
        ];
        let Resolution::Ambiguous(matches) = match_query(candidates.clone(), "WEB") else {
            panic!("two names differing only by case are ambiguous");
        };
        assert_eq!(matches.len(), 2);
        assert!(matches!(match_query(candidates, ""), Resolution::NotFound(all) if all.len() == 3));
    }

    #[test]
    fn typed_references_match_databases() {
        let tenant: Tenant = serde_json::from_value(json!({"id": "t-1", "name": "app"})).unwrap();
        let candidates = vec![Database::Tenant(tenant)];
        assert!(matches!(
            match_query(candidates.clone(), "tenant:t-1"),
            Resolution::Found(_)
        ));
        assert!(matches!(
            match_query(candidates, "app"),
            Resolution::Found(_)
        ));
    }

    #[test]
    fn candidates_carry_the_argument_to_pass() {
        let tenant: Tenant = serde_json::from_value(json!({"id": "t-1", "name": "app"})).unwrap();
        let candidate = Candidate::from(&Database::Tenant(tenant));
        assert_eq!(candidate.id, "tenant:t-1");
        assert_eq!(candidate.name, "app");
    }

    fn enterprise(
        database: &str,
        project: Option<&str>,
        workspace: Option<&str>,
    ) -> EnterpriseInstanceConfig {
        EnterpriseInstanceConfig {
            database: database.parse().unwrap(),
            project_id: project.map(str::to_owned),
            workspace_id: workspace.map(str::to_owned),
        }
    }

    #[test]
    fn link_prefers_the_project_table() {
        let mut config = HelixConfig::default_config("p");
        config.project.id = Some("from-project".into());
        config.project.workspace_id = Some("ws".into());
        config.enterprise.insert(
            "prod".into(),
            enterprise("tenant:t", Some("from-instance"), None),
        );
        let link = Link::from_config(&config);
        assert_eq!(
            link.project,
            Some(ProjectLink {
                project_id: "from-project".into(),
                workspace_id: Some("ws".into()),
            })
        );
        assert_eq!(link.databases, [DatabaseReference::Tenant("t".into())]);
    }

    #[test]
    fn link_falls_back_to_a_single_shared_instance_owner() {
        let mut config = HelixConfig::default_config("p");
        config
            .enterprise
            .insert("a".into(), enterprise("tenant:t1", Some("p1"), Some("w1")));
        config
            .enterprise
            .insert("b".into(), enterprise("tenant:t1", Some("p1"), Some("w1")));
        let link = Link::from_config(&config);
        assert_eq!(link.project.unwrap().project_id, "p1");
        assert_eq!(link.databases.len(), 1, "duplicate databases collapse");

        config
            .enterprise
            .insert("c".into(), enterprise("cluster:c1", Some("p2"), Some("w1")));
        let link = Link::from_config(&config);
        assert_eq!(link.project, None, "owners disagree, so nothing is linked");
        assert_eq!(link.databases.len(), 2);
    }

    #[test]
    fn unlinked_configs_have_no_link() {
        assert_eq!(
            Link::from_config(&HelixConfig::default_config("p")),
            Link::default()
        );
    }

    fn offline_scope(interactive: bool) -> Scope {
        Scope {
            client: CloudClient::with_paths(
                "http://127.0.0.1:9".into(),
                std::env::temp_dir().join("helix-resolve-test-credentials"),
            )
            .unwrap(),
            link: Link::default(),
            interactive,
        }
    }

    #[test]
    fn choose_takes_the_only_candidate_and_lists_many_when_it_cannot_prompt() {
        let scope = offline_scope(false);
        let only = scope
            .choose(Kind::Project, vec![project("p-1", "a", "A")])
            .unwrap();
        assert_eq!(only.id, "p-1");

        let error = scope
            .choose(
                Kind::Project,
                vec![project("p-1", "a", "A"), project("p-2", "b", "B")],
            )
            .unwrap_err();
        let error = error.downcast_ref::<CliError>().unwrap();
        assert_eq!(error.message, "2 projects found; choose one");
        assert_eq!(error.candidates.len(), 2);
        assert!(error.hint.as_deref().unwrap().contains("--project"));

        let error = scope
            .choose(Kind::Database, Vec::<Database>::new())
            .unwrap_err();
        assert!(error
            .downcast_ref::<CliError>()
            .unwrap()
            .hint
            .as_deref()
            .unwrap()
            .contains("helix database create"));
    }

    #[test]
    fn pick_explains_ambiguous_and_missing_matches() {
        let scope = offline_scope(false);
        let candidates = vec![project("p-1", "", "Web"), project("p-2", "", "web")];
        let error = scope
            .pick(Kind::Project, "web", candidates.clone())
            .unwrap_err();
        assert_eq!(
            error.downcast_ref::<CliError>().unwrap().message,
            "'web' matches 2 projects"
        );
        let error = scope.pick(Kind::Project, "api", candidates).unwrap_err();
        let error = error.downcast_ref::<CliError>().unwrap();
        assert_eq!(error.message, "no project matches 'api'");
        assert_eq!(error.candidates.len(), 2);
    }

    #[test]
    fn stale_links_point_at_the_fix() {
        let error = stale_link(Kind::Project, "p-9", &eyre::eyre!("HTTP 404"));
        let error = error.downcast_ref::<CliError>().unwrap();
        assert!(error.message.contains("p-9"));
        assert!(error
            .hint
            .as_deref()
            .unwrap()
            .contains("helix project link"));
        let error = stale_link(Kind::Database, "tenant:t", &eyre::eyre!("HTTP 404"));
        assert!(error
            .downcast_ref::<CliError>()
            .unwrap()
            .hint
            .as_deref()
            .unwrap()
            .contains("helix add cloud"));
    }
}
