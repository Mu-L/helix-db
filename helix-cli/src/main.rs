use clap::{Parser, Subcommand};
use color_eyre::owo_colors::OwoColorize;
use eyre::Result;
use helix_cli::{
    AddTarget, AuthAction, ClusterConfigAction, ConfigAction, DashboardAction, InitTarget,
    MetricsAction, ProjectConfigAction, WorkspaceConfigAction, commands, errors, metrics_sender,
    output, update,
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
        target: Option<AddTarget>,
    },

    /// Run a local v2 instance in the background
    Run {
        /// Instance name to run
        instance: Option<String>,
        /// Run in the foreground and stop on Ctrl-C
        #[arg(long, conflicts_with = "detach")]
        foreground: bool,
        /// Run in the background (default)
        #[arg(long, hide = true)]
        detach: bool,
        /// Override local port for this run
        #[arg(long)]
        port: Option<u16>,
        /// Use on-disk storage backed by a local MinIO container for this run
        #[arg(long)]
        disk: bool,
    },

    /// Stop a background local v2 instance
    Stop {
        /// Instance name to stop
        instance: Option<String>,
    },

    /// Restart a background local v2 instance
    Restart {
        /// Instance name to restart
        instance: Option<String>,
    },

    /// Show local and Enterprise Cloud instance status
    Status {
        /// Instance name to show, defaults to all instances
        instance: Option<String>,
    },

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
        #[arg(
            short,
            long,
            value_name = "REQUEST.json",
            conflicts_with = "json",
            required_unless_present = "json"
        )]
        file: Option<String>,
        /// JSON request body
        #[arg(
            long,
            value_name = "JSON",
            conflicts_with = "file",
            required_unless_present = "file"
        )]
        json: Option<String>,
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

    /// Deploy an Enterprise Cloud instance
    Push {
        /// Enterprise instance name to deploy
        instance: Option<String>,
        /// Deprecated Helix Cloud dev deploy override; ignored for Enterprise deploys
        #[arg(long, hide = true)]
        dev: bool,
    },

    /// Enterprise Cloud auth operations
    Auth {
        #[command(subcommand)]
        action: AuthAction,
    },

    /// Configure workspace, project, and Enterprise cluster defaults
    #[command(hide = true)]
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },

    /// Manage active Enterprise Cloud workspace selection
    Workspace {
        #[command(subcommand)]
        action: Option<WorkspaceConfigAction>,
    },

    /// Manage linked Enterprise Cloud project selection
    Project {
        #[command(subcommand)]
        action: Option<ProjectConfigAction>,
    },

    /// List and inspect Enterprise Cloud clusters
    Cluster {
        #[command(subcommand)]
        action: Option<ClusterConfigAction>,
    },

    /// Sync Enterprise Cloud metadata into helix.toml
    Sync {
        /// Enterprise instance name
        instance: Option<String>,
        /// Overwrite local/remote source during reconciliation without confirmation prompts
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// Prune local v2 containers/workspaces
    Prune {
        /// Instance to prune
        instance: Option<String>,
        /// Prune all local instances
        #[arg(short, long)]
        all: bool,
        /// Skip confirmation prompts
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// Delete an instance from helix.toml and local runtime state
    Delete {
        /// Instance name to delete
        instance: String,
        /// Skip confirmation prompts
        #[arg(short = 'y', long)]
        yes: bool,
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
        /// Update to the last v1-compatible CLI version
        #[arg(long)]
        v1: bool,
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
    let padded = format!("{cmd:<38}");
    if use_color {
        println!(
            "  {} {}",
            padded.truecolor(255, 165, 54).bold(),
            desc.dimmed()
        );
    } else {
        println!("  {padded} {desc}");
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
            foreground,
            detach: _,
            port,
            disk,
        }) => commands::run::run(instance, foreground, port, disk).await,
        Some(Commands::Stop { instance }) => commands::stop::run(instance).await,
        Some(Commands::Restart { instance }) => commands::restart::run(instance).await,
        Some(Commands::Status { instance }) => commands::status::run(instance).await,
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
            json,
            warm,
            host,
            port,
            compact,
        }) => commands::query::run(instance, file, json, warm, host, port, compact).await,
        Some(Commands::Push { instance, dev }) => {
            commands::push::run(instance, dev, &metrics_sender).await
        }
        Some(Commands::Auth { action }) => commands::auth::run(action).await,
        Some(Commands::Config { action }) => commands::config::run(action).await,
        Some(Commands::Workspace { action }) => commands::config::run_workspace(action).await,
        Some(Commands::Project { action }) => commands::config::run_project(action).await,
        Some(Commands::Cluster { action }) => commands::config::run_cluster(action).await,
        Some(Commands::Sync { instance, yes }) => commands::sync::run(instance, yes).await,
        Some(Commands::Prune { instance, all, yes }) => {
            commands::prune::run(instance, all, yes).await
        }
        Some(Commands::Delete { instance, yes }) => commands::delete::run(instance, yes).await,
        Some(Commands::Metrics { action }) => commands::metrics::run(action).await,
        Some(Commands::Dashboard { action }) => commands::dashboard::run(action).await,
        Some(Commands::Update { force, v1 }) => commands::update::run(force, v1).await,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_defaults_to_background() {
        let cli = Cli::parse_from(["helix", "run", "qa"]);

        match cli.command {
            Some(Commands::Run {
                instance,
                foreground,
                detach,
                port,
                disk,
            }) => {
                assert_eq!(instance.as_deref(), Some("qa"));
                assert!(!foreground);
                assert!(!detach);
                assert_eq!(port, None);
                assert!(!disk);
            }
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn run_foreground_flag_enables_attached_mode() {
        let cli = Cli::parse_from(["helix", "run", "qa", "--foreground"]);

        match cli.command {
            Some(Commands::Run { foreground, .. }) => assert!(foreground),
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn run_disk_flag_enables_on_disk_mode() {
        let cli = Cli::parse_from(["helix", "run", "qa", "--disk"]);

        match cli.command {
            Some(Commands::Run { disk, .. }) => assert!(disk),
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn run_detach_flag_remains_background_alias() {
        let cli = Cli::parse_from(["helix", "run", "qa", "--detach"]);

        match cli.command {
            Some(Commands::Run {
                foreground, detach, ..
            }) => {
                assert!(!foreground);
                assert!(detach);
            }
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn run_foreground_conflicts_with_detach_alias() {
        assert!(Cli::try_parse_from(["helix", "run", "qa", "--foreground", "--detach"]).is_err());
    }

    #[test]
    fn init_local_disk_flag_parses() {
        let cli = Cli::parse_from(["helix", "init", "local", "--disk"]);

        match cli.command {
            Some(Commands::Init {
                target: Some(InitTarget::Local { name, port, disk }),
                ..
            }) => {
                assert_eq!(name, "dev");
                assert_eq!(port, helix_cli::config::DEFAULT_LOCAL_PORT);
                assert!(disk);
            }
            _ => panic!("expected init local command"),
        }
    }

    #[test]
    fn add_local_disk_flag_parses() {
        let cli = Cli::parse_from(["helix", "add", "local", "--name", "qa", "--disk"]);

        match cli.command {
            Some(Commands::Add {
                target: Some(AddTarget::Local { name, port, disk }),
            }) => {
                assert_eq!(name, "qa");
                assert_eq!(port, helix_cli::config::DEFAULT_LOCAL_PORT);
                assert!(disk);
            }
            _ => panic!("expected add local command"),
        }
    }

    #[test]
    fn update_v1_flag_parses() {
        let cli = Cli::parse_from(["helix", "update", "--v1"]);

        match cli.command {
            Some(Commands::Update { force, v1 }) => {
                assert!(!force);
                assert!(v1);
            }
            _ => panic!("expected update command"),
        }
    }

    #[test]
    fn add_allows_interactive_entrypoint() {
        let cli = Cli::parse_from(["helix", "add"]);

        match cli.command {
            Some(Commands::Add { target }) => assert!(target.is_none()),
            _ => panic!("expected add command"),
        }
    }

    #[test]
    fn root_workspace_command_parses() {
        let cli = Cli::parse_from(["helix", "workspace", "list"]);

        match cli.command {
            Some(Commands::Workspace {
                action: Some(WorkspaceConfigAction::List { .. }),
            }) => {}
            _ => panic!("expected workspace list command"),
        }
    }

    #[test]
    fn root_project_command_parses() {
        let cli = Cli::parse_from(["helix", "project", "show"]);

        match cli.command {
            Some(Commands::Project {
                action: Some(ProjectConfigAction::Show { .. }),
            }) => {}
            _ => panic!("expected project show command"),
        }
    }

    #[test]
    fn root_cluster_command_parses() {
        let cli = Cli::parse_from(["helix", "cluster", "list"]);

        match cli.command {
            Some(Commands::Cluster {
                action: Some(ClusterConfigAction::List { .. }),
            }) => {}
            _ => panic!("expected cluster list command"),
        }
    }

    #[test]
    fn status_accepts_optional_instance() {
        let cli = Cli::parse_from(["helix", "status", "qa"]);

        match cli.command {
            Some(Commands::Status { instance }) => assert_eq!(instance.as_deref(), Some("qa")),
            _ => panic!("expected status command"),
        }
    }

    #[test]
    fn query_accepts_file_input() {
        let cli = Cli::parse_from(["helix", "query", "dev", "--file", "request.json"]);

        match cli.command {
            Some(Commands::Query { file, json, .. }) => {
                assert_eq!(file.as_deref(), Some("request.json"));
                assert!(json.is_none());
            }
            _ => panic!("expected query command"),
        }
    }

    #[test]
    fn query_accepts_inline_json_input() {
        let inline_json = r#"{"request_type":"read","query":{"queries":[]}}"#;
        let cli = Cli::parse_from(["helix", "query", "dev", "--json", inline_json]);

        match cli.command {
            Some(Commands::Query { file, json, .. }) => {
                assert!(file.is_none());
                assert_eq!(json.as_deref(), Some(inline_json));
            }
            _ => panic!("expected query command"),
        }
    }

    #[test]
    fn query_rejects_missing_input() {
        assert!(Cli::try_parse_from(["helix", "query", "dev"]).is_err());
    }

    #[test]
    fn query_rejects_file_and_inline_json_together() {
        assert!(
            Cli::try_parse_from([
                "helix",
                "query",
                "dev",
                "--file",
                "request.json",
                "--json",
                "{}",
            ])
            .is_err()
        );
    }

    #[test]
    fn push_accepts_optional_enterprise_instance() {
        let cli = Cli::parse_from(["helix", "push", "production"]);

        match cli.command {
            Some(Commands::Push { instance, dev }) => {
                assert_eq!(instance.as_deref(), Some("production"));
                assert!(!dev);
            }
            _ => panic!("expected push command"),
        }
    }

    #[test]
    fn sync_accepts_yes_for_noninteractive_reconciliation() {
        let cli = Cli::parse_from(["helix", "sync", "production", "--yes"]);

        match cli.command {
            Some(Commands::Sync { instance, yes }) => {
                assert_eq!(instance.as_deref(), Some("production"));
                assert!(yes);
            }
            _ => panic!("expected sync command"),
        }
    }
}
