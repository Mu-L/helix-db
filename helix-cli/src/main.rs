use clap::{Parser, Subcommand};
use eyre::Result;

use crate::utils::{DeploymentType, Template};

mod commands;
mod config;
mod docker;
mod project;
mod update;
mod utils;

#[derive(Parser)]
#[command(name = "Helix CLI")]
#[command(version)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new Helix project with helix.toml
    Init {
        /// Project directory (defaults to current directory)
        #[clap(short, long)]
        path: Option<String>,

        #[clap(long)]
        template: Option<String>,

        #[clap(long, default_value = "helix")]
        cloud: Option<String>,
    },

    /// Validate project configuration and queries
    Check {
        /// Instance to check (defaults to all instances)
        instance: Option<String>,
    },

    /// Build and compile project for an instance
    Build {
        /// Instance name to build
        instance: String,
    },

    /// Deploy/start an instance
    Push {
        /// Instance name to push
        instance: String,
    },

    /// Pull .hql files from instance back to local project
    Pull {
        /// Instance name to pull from
        instance: String,
    },

    /// Show status of all instances
    Status,

    /// Cloud operations (login, keys, etc.)
    Cloud {
        #[clap(subcommand)]
        action: CloudAction,
    },

    /// Clean up containers and volumes
    Prune {
        /// Instance to prune (if not specified, prunes unused resources)
        instance: Option<String>,

        /// Remove all instance data
        #[clap(short, long)]
        all: bool,
    },

    /// Delete an instance completely
    Delete {
        /// Instance name to delete
        instance: String,
    },

    /// Manage metrics collection
    Metrics {
        #[clap(subcommand)]
        action: MetricsAction,
    },
}

#[derive(Subcommand)]
enum CloudAction {
    /// Login to Helix cloud
    Login,
    /// Logout from Helix cloud
    Logout,
    /// Create a new API key
    CreateKey {
        /// Cluster ID
        cluster: String,
    },
}

#[derive(Subcommand)]
enum MetricsAction {
    /// Enable metrics collection
    On,
    /// Disable metrics collection
    Off,
    /// Show metrics status
    Status,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize error reporting
    color_eyre::install()?;

    // Check for updates before processing commands
    update::check_for_updates().await?;

    let cli = Cli::parse();

    match cli.command {
        Commands::Init {
            path,
            template,
            cloud,
        } => {
            commands::init::run(
                path,
                template.map(|t| Template::from_str(&t)).transpose()?,
                cloud.map(|c| DeploymentType::from_str(&c)).transpose()?,
            )
            .await
        }
        Commands::Check { instance } => commands::check::run(instance).await,
        Commands::Build { instance } => commands::build::run(instance).await,
        Commands::Push { instance } => commands::push::run(instance).await,
        Commands::Pull { instance } => commands::pull::run(instance).await,
        Commands::Status => commands::status::run().await,
        Commands::Cloud { action } => commands::cloud::run(action).await,
        Commands::Prune { instance, all } => commands::prune::run(instance, all).await,
        Commands::Delete { instance } => commands::delete::run(instance).await,
        Commands::Metrics { action } => commands::metrics::run(action).await,
    }
}
