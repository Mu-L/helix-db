use clap::{ArgGroup, Parser, Subcommand};
use eyre::Result;
use std::panic;

mod commands;
mod config;
mod docker;
mod errors;
mod metrics_sender;
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
    #[command(group = ArgGroup::new("init_type").multiple(false))]
    Init {
        /// Project directory (defaults to current directory)
        #[clap(short, long)]
        path: Option<String>,

        #[clap(long, default_value = "empty")]
        template: String,

        /// Queries directory path (defaults to ./db/)
        #[clap(short = 'q', long = "queries-path", default_value = "./db/")]
        queries_path: String,

        /// Initialize with Helix cloud instance
        #[arg(long, group = "init_type")]
        cloud: bool,

        /// Region for Helix cloud instance (default: us-east-1)
        #[arg(long, value_name = "REGION", requires = "cloud")]
        cloud_region: Option<String>,

        /// Initialize with AWS ECR instance
        #[arg(long, group = "init_type")]
        ecr: bool,

        /// Initialize with Fly.io instance
        #[arg(long, group = "init_type")]
        fly: bool,

        /// Authentication type for Fly.io (default: cli)
        #[arg(long, value_name = "TYPE", default_value = "cli", requires = "fly")]
        fly_auth: String,

        /// Volume size in GB for Fly.io (default: 20)
        #[arg(long, value_name = "GB", default_value_t = 20, requires = "fly")]
        fly_volume_size: u16,

        /// VM size for Fly.io (default: shared-cpu-4x)
        #[arg(long, value_name = "SIZE", default_value = "shared-cpu-4x", requires = "fly")]
        fly_vm_size: String,

        /// Make Fly.io instance public (default: true)
        #[arg(long, default_value_t = true, requires = "fly")]
        fly_public: bool,
    },

    /// Add a new instance to an existing Helix project
    #[command(group = ArgGroup::new("instance_type").multiple(false))]
    Add {
        /// Instance name
        name: String,

        /// Add a Helix cloud instance
        #[arg(long, group = "instance_type")]
        cloud: bool,

        /// Region for Helix cloud instance (default: us-east-1)
        #[arg(long, value_name = "REGION", requires = "cloud")]
        cloud_region: Option<String>,

        /// Add an AWS ECR instance
        #[arg(long, group = "instance_type")]
        ecr: bool,

        /// Add a Fly.io instance
        #[arg(long, group = "instance_type")]
        fly: bool,

        /// Authentication type for Fly.io (default: cli)
        #[arg(long, value_name = "TYPE", default_value = "cli", requires = "fly")]
        fly_auth: String,

        /// Volume size in GB for Fly.io (default: 20)
        #[arg(long, value_name = "GB", default_value_t = 20, requires = "fly")]
        fly_volume_size: u16,

        /// VM size for Fly.io (default: shared-cpu-4x)
        #[arg(long, value_name = "SIZE", default_value = "shared-cpu-4x", requires = "fly")]
        fly_vm_size: String,

        /// Make Fly.io instance public (default: true)
        #[arg(long, default_value_t = true, requires = "fly")]
        fly_public: bool,
    },

    /// Validate project configuration and queries
    Check {
        /// Instance to check (defaults to all instances)
        instance: Option<String>,
    },

    /// Compile project queries into the workspace
    Compile {
        /// Path to output directory
        #[clap(short, long)]
        path: Option<String>,

        /// Instance name to compile
        #[clap(short, long)]
        output: Option<String>,
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

    /// Start an instance (doesn't rebuild)
    Start {
        /// Instance name to start
        instance: String,
    },

    /// Stop an instance
    Stop {
        /// Instance name to stop
        instance: String,
    },

    /// Show status of all instances
    Status,

    /// Cloud operations (login, keys, etc.)
    Auth {
        #[clap(subcommand)]
        action: AuthAction,
    },

    /// Prune containers, images and workspace (preserves volumes)
    Prune {
        /// Instance to prune (if not specified, prunes unused resources)
        instance: Option<String>,

        /// Prune all instances in project
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

    /// Update to the latest version
    Update {
        /// Force update even if already on latest version
        #[clap(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum AuthAction {
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
    Full,
    /// Disable metrics collection
    Basic,
    /// Disable metrics collection
    Off,
    /// Show metrics status
    Status,
}


#[tokio::main]
async fn main() -> Result<()> {
    // Initialize error reporting
    color_eyre::install()?;

    // Install custom panic hook for better user experience
    panic::set_hook(Box::new(|panic_info| {
        let msg = match panic_info.payload().downcast_ref::<&str>() {
            Some(s) => *s,
            None => match panic_info.payload().downcast_ref::<String>() {
                Some(s) => s.as_str(),
                None => "An unexpected error occurred",
            }
        };

        let location = if let Some(location) = panic_info.location() {
            format!("{}:{}", location.file(), location.line())
        } else {
            "unknown location".to_string()
        };

        let error = crate::errors::CliError::new("Helix CLI encountered an unexpected error")
            .with_context(msg)
            .with_hint("This is likely a bug. Please report it at https://github.com/HelixDB/helix-db/issues");

        eprint!("{}", error.render());
        eprintln!("   = note: error occurred at {location}");
        std::process::exit(1);
    }));

    // Initialize metrics sender
    let metrics_sender = metrics_sender::MetricsSender::new()?;

    // Send CLI install event (only first time)
    metrics_sender.send_cli_install_event_if_first_time();

    // Check for updates before processing commands
    update::check_for_updates().await?;

    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Init {
            path,
            template,
            queries_path,
            cloud,
            cloud_region,
            ecr,
            fly,
            fly_auth,
            fly_volume_size,
            fly_vm_size,
            fly_public,
        } => commands::init::run(
            path, 
            template, 
            queries_path, 
            cloud,
            cloud_region,
            ecr,
            fly,
            fly_auth,
            fly_volume_size,
            fly_vm_size,
            fly_public,
        ).await,
        Commands::Add { 
            name, 
            cloud, 
            cloud_region,
            ecr, 
            fly,
            fly_auth,
            fly_volume_size,
            fly_vm_size,
            fly_public,
        } => commands::add::run(
            name, 
            cloud, 
            cloud_region,
            ecr, 
            fly,
            fly_auth,
            fly_volume_size,
            fly_vm_size,
            fly_public,
        ).await,
        Commands::Check { instance } => commands::check::run(instance).await,
        Commands::Compile { output, path } => commands::compile::run(output, path).await,
        Commands::Build { instance } => commands::build::run(instance, &metrics_sender)
            .await
            .map(|_| ()),
        Commands::Push { instance } => commands::push::run(instance, &metrics_sender).await,
        Commands::Pull { instance } => commands::pull::run(instance).await,
        Commands::Start { instance } => commands::start::run(instance).await,
        Commands::Stop { instance } => commands::stop::run(instance).await,
        Commands::Status => commands::status::run().await,
        Commands::Auth { action } => commands::auth::run(action).await,
        Commands::Prune { instance, all } => commands::prune::run(instance, all).await,
        Commands::Delete { instance } => commands::delete::run(instance).await,
        Commands::Metrics { action } => commands::metrics::run(action).await,
        Commands::Update { force } => commands::update::run(force).await,
    };

    // Shutdown metrics sender
    metrics_sender.shutdown().await?;

    // Handle result with proper error formatting
    if let Err(e) = result {
        // Try to convert eyre error to CliError for better formatting
        let cli_error = crate::errors::CliError::new(&e.to_string())
            .with_hint("Run 'helix --help' for usage information");
        eprint!("{}", cli_error.render());
        std::process::exit(1);
    }

    Ok(())
}
