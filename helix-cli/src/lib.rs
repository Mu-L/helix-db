use clap::{Subcommand, ValueEnum};

pub mod commands;
pub mod config;
pub mod enterprise_cloud;
pub mod errors;
pub mod local_runtime;
pub mod metrics_sender;
pub mod output;
pub mod port;
pub mod project;
pub mod prompts;
pub mod setup;
pub mod sse_client;
pub mod update;
pub mod utils;

#[derive(Subcommand)]
pub enum AuthAction {
    /// Login to Helix Cloud
    Login,
    /// Logout from Helix Cloud
    Logout,
    /// Rotate an Enterprise cluster API key
    CreateKey {
        /// Cluster ID
        cluster: String,
    },
}

#[derive(Subcommand)]
pub enum InitTarget {
    /// Initialize a local v2 development project
    Local {
        /// Local instance name
        #[arg(short, long, default_value = "dev")]
        name: String,
        /// Local gateway port
        #[arg(long, default_value_t = crate::config::DEFAULT_LOCAL_PORT)]
        port: u16,
        /// Use on-disk storage backed by a local MinIO container
        #[arg(long)]
        disk: bool,
    },
    /// Initialize an Enterprise Cloud project
    Enterprise {
        /// Enterprise instance name
        #[arg(short, long, default_value = "production")]
        name: String,
        /// Enterprise cluster ID
        #[arg(long)]
        cluster_id: String,
        /// Runtime gateway URL for dynamic queries
        #[arg(long)]
        gateway_url: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum AddTarget {
    /// Add a local v2 development instance
    Local {
        /// Local instance name
        #[arg(short, long)]
        name: String,
        /// Local gateway port
        #[arg(long, default_value_t = crate::config::DEFAULT_LOCAL_PORT)]
        port: u16,
        /// Use on-disk storage backed by a local MinIO container
        #[arg(long)]
        disk: bool,
    },
    /// Add an Enterprise Cloud instance
    Enterprise {
        /// Enterprise instance name
        #[arg(short, long)]
        name: String,
        /// Enterprise cluster ID
        #[arg(long)]
        cluster_id: String,
        /// Runtime gateway URL for dynamic queries
        #[arg(long)]
        gateway_url: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum MetricsAction {
    /// Enable full metrics collection
    Full,
    /// Enable basic metrics collection
    Basic,
    /// Disable metrics collection
    Off,
    /// Show metrics status
    Status,
}

#[derive(Subcommand)]
pub enum DashboardAction {
    /// Start the dashboard
    Start {
        /// Instance to connect to
        instance: Option<String>,
        /// Port to run dashboard on
        #[arg(short, long, default_value = "3000")]
        port: u16,
        /// Helix host to connect to
        #[arg(long)]
        host: Option<String>,
        /// Helix port to connect to
        #[arg(long, default_value_t = crate::config::DEFAULT_LOCAL_PORT)]
        helix_port: u16,
        /// Run dashboard in foreground with logs
        #[arg(long)]
        attach: bool,
        /// Restart if dashboard is already running
        #[arg(long)]
        restart: bool,
    },
    /// Stop the dashboard
    Stop,
    /// Show dashboard status
    Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum ConfigOutputFormat {
    #[default]
    Human,
    Json,
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Manage active workspace selection
    Workspace {
        #[command(subcommand)]
        action: WorkspaceConfigAction,
    },
    /// Manage linked project selection
    Project {
        #[command(subcommand)]
        action: ProjectConfigAction,
    },
    /// List Enterprise clusters
    Cluster {
        #[command(subcommand)]
        action: ClusterConfigAction,
    },
}

#[derive(Subcommand)]
pub enum WorkspaceConfigAction {
    /// List accessible workspaces
    List {
        #[arg(long, value_enum, default_value_t = ConfigOutputFormat::Human)]
        format: ConfigOutputFormat,
    },
    /// Show selected workspace
    Show {
        #[arg(long, value_enum, default_value_t = ConfigOutputFormat::Human)]
        format: ConfigOutputFormat,
    },
    /// Select workspace by slug or ID
    Switch {
        workspace: String,
        #[arg(long)]
        id: bool,
    },
}

#[derive(Subcommand)]
pub enum ProjectConfigAction {
    /// List projects in the selected workspace
    List {
        #[arg(long)]
        workspace_id: Option<String>,
        #[arg(long, value_enum, default_value_t = ConfigOutputFormat::Human)]
        format: ConfigOutputFormat,
    },
    /// Show linked project
    Show {
        #[arg(long, value_enum, default_value_t = ConfigOutputFormat::Human)]
        format: ConfigOutputFormat,
    },
    /// Link this project to a cloud project by name or ID
    Switch {
        project: String,
        #[arg(long)]
        id: bool,
    },
}

#[derive(Subcommand)]
pub enum ClusterConfigAction {
    /// List Enterprise clusters
    List {
        #[arg(long)]
        workspace_id: Option<String>,
        #[arg(long)]
        project_id: Option<String>,
        #[arg(long, value_enum, default_value_t = ConfigOutputFormat::Human)]
        format: ConfigOutputFormat,
    },
}
