use clap::{Parser, Subcommand};
use color_eyre::owo_colors::OwoColorize;
use eyre::Result;
use helix_cli::{
    AddTarget, AuthAction, ConfigAction, DashboardAction, InitTarget, MetricsAction, commands,
    errors, metrics_sender, output, update,
};
use std::io::IsTerminal;
use tui_banner::{Align, Banner, ColorMode, Fill, Gradient, Palette};

#[derive(Parser)]
#[command(name = "Helix CLI")]
#[command(version)]
struct Cli {
    /// Suppress output (errors and final result only)
    #[arg(long, global = true)]
    quiet: bool,

    /// Show detailed output with timing information
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a v2 Helix project
    Init {
        /// Project directory (defaults to current directory)
        #[arg(short, long)]
        path: Option<String>,
        #[command(subcommand)]
        target: Option<InitTarget>,
    },

    /// Add a local v2 or Enterprise Cloud instance
    Add {
        #[command(subcommand)]
        target: AddTarget,
    },

    /// Run a local v2 instance
    Run {
        /// Instance name to run
        instance: Option<String>,
        /// Run in the background
        #[arg(long)]
        detach: bool,
        /// Override local port for this run
        #[arg(long)]
        port: Option<u16>,
    },

    /// Stop a detached local v2 instance
    Stop {
        /// Instance name to stop
        instance: Option<String>,
    },

    /// Restart a detached local v2 instance
    Restart {
        /// Instance name to restart
        instance: Option<String>,
    },

    /// Show local and Enterprise Cloud instance status
    Status,

    /// View logs for a local or Enterprise Cloud instance
    Logs {
        /// Instance name
        instance: Option<String>,
        /// Follow logs
        #[arg(long, short = 'f')]
        follow: bool,
        /// Query historical logs with time range for Enterprise Cloud
        #[arg(long, short = 'r')]
        range: bool,
        /// Start time (ISO 8601)
        #[arg(long, requires = "range")]
        start: Option<String>,
        /// End time (ISO 8601)
        #[arg(long, requires = "range")]
        end: Option<String>,
    },

    /// Send a dynamic query to POST /v1/query
    Query {
        /// Instance name
        instance: Option<String>,
        /// JSON request file
        #[arg(short, long)]
        file: String,
        /// Add X-Helix-Warm header. Only valid for read requests.
        #[arg(long)]
        warm: bool,
        /// Override host for local query execution
        #[arg(long)]
        host: Option<String>,
        /// Override port for local query execution
        #[arg(long)]
        port: Option<u16>,
        /// Print compact JSON instead of pretty JSON
        #[arg(long)]
        compact: bool,
    },

    /// Enterprise Cloud auth operations
    Auth {
        #[command(subcommand)]
        action: AuthAction,
    },

    /// Configure workspace, project, and Enterprise cluster defaults
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Sync Enterprise Cloud metadata into helix.toml
    Sync {
        /// Enterprise instance name
        instance: Option<String>,
    },

    /// Prune local v2 containers/workspaces
    Prune {
        /// Instance to prune
        instance: Option<String>,
        /// Prune all local instances
        #[arg(short, long)]
        all: bool,
    },

    /// Delete an instance from helix.toml and local runtime state
    Delete {
        /// Instance name to delete
        instance: String,
    },

    /// Manage metrics collection
    Metrics {
        #[command(subcommand)]
        action: MetricsAction,
    },

    /// Launch the Helix Dashboard
    Dashboard {
        #[command(subcommand)]
        action: DashboardAction,
    },

    /// Update to the latest CLI version
    Update {
        /// Force update even if already on latest version
        #[arg(long)]
        force: bool,
    },

    /// Send feedback to the Helix team
    Feedback {
        /// Feedback message
        message: Option<String>,
    },
}

fn display_welcome(update_available: Option<String>) {
    let use_color = std::io::stdout().is_terminal();

    if let Ok(banner) = Banner::new("> HELIX DB") {
        let banner = banner
            .color_mode(ColorMode::TrueColor)
            .gradient(Gradient::vertical(Palette::from_hex(&[
                "#ff7f17", "#e36600", "#8f4000",
            ])))
            .fill(Fill::Keep)
            .dither()
            .targets("░▒▓")
            .checker(3)
            .align(Align::Center)
            .padding(3)
            .render();
        println!("{banner}");
    }

    let version = update::current_version();
    if use_color {
        println!(
            "  {} {}\n",
            "Helix DB CLI".bold(),
            format!("v{}", version).dimmed()
        );
    } else {
        println!("  Helix DB CLI v{}\n", version);
    }

    if let Some(latest_version) = update_available {
        println!("  Update available: v{} -> v{}", version, latest_version);
        println!("  Run 'helix update' to upgrade\n");
    }

    println!(
        "{}",
        if use_color {
            "Getting Started".bold().to_string()
        } else {
            "Getting Started".to_string()
        }
    );
    println!();
    print_command("helix init", "Create a v2 project", use_color);
    print_command("helix run dev", "Run local Enterprise dev", use_color);
    print_command(
        "helix query dev --file request.json",
        "Send a dynamic query",
        use_color,
    );
    print_command("helix auth login", "Login to Enterprise Cloud", use_color);
    println!();
    println!("Docs: https://docs.helix-db.com");
}

fn print_command(cmd: &str, desc: &str, use_color: bool) {
    if use_color {
        println!(
            "  {}  {}",
            cmd.truecolor(255, 165, 54).bold(),
            desc.dimmed()
        );
    } else {
        println!("  {:34} {}", cmd, desc);
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let metrics_sender = metrics_sender::MetricsSender::new()?;
    metrics_sender.send_cli_install_event_if_first_time();
    let update_available = update::check_for_updates().await?;

    let cli = Cli::parse();
    output::Verbosity::set(output::Verbosity::from_flags(cli.quiet, cli.verbose));

    let result = match cli.command {
        None => {
            display_welcome(update_available);
            Ok(())
        }
        Some(Commands::Init { path, target }) => commands::init::run(path, target).await,
        Some(Commands::Add { target }) => commands::add::run(target).await,
        Some(Commands::Run {
            instance,
            detach,
            port,
        }) => commands::run::run(instance, detach, port).await,
        Some(Commands::Stop { instance }) => commands::stop::run(instance).await,
        Some(Commands::Restart { instance }) => commands::restart::run(instance).await,
        Some(Commands::Status) => commands::status::run().await,
        Some(Commands::Logs {
            instance,
            follow,
            range,
            start,
            end,
        }) => commands::logs::run(instance, follow, range, start, end).await,
        Some(Commands::Query {
            instance,
            file,
            warm,
            host,
            port,
            compact,
        }) => commands::query::run(instance, file, warm, host, port, compact).await,
        Some(Commands::Auth { action }) => commands::auth::run(action).await,
        Some(Commands::Config { action }) => commands::config::run(action).await,
        Some(Commands::Sync { instance }) => commands::sync::run(instance).await,
        Some(Commands::Prune { instance, all }) => commands::prune::run(instance, all).await,
        Some(Commands::Delete { instance }) => commands::delete::run(instance).await,
        Some(Commands::Metrics { action }) => commands::metrics::run(action).await,
        Some(Commands::Dashboard { action }) => commands::dashboard::run(action).await,
        Some(Commands::Update { force }) => commands::update::run(force).await,
        Some(Commands::Feedback { message }) => commands::feedback::run(message).await,
    };

    metrics_sender.shutdown().await?;

    if let Err(e) = result {
        if let Some(cli_error) = e.downcast_ref::<errors::CliError>() {
            eprint!("{}", cli_error.render());
        } else if let Some(config_error) = e.downcast_ref::<errors::ConfigError>() {
            eprint!("{}", config_error.to_cli_error().render());
        } else if let Some(project_error) = e.downcast_ref::<errors::ProjectError>() {
            eprint!("{}", project_error.to_cli_error().render());
        } else if let Some(port_error) = e.downcast_ref::<errors::PortError>() {
            eprint!("{}", port_error.to_cli_error().render());
        } else {
            eprintln!("{e}");
        }
        std::process::exit(1);
    }

    Ok(())
}
