use crate::config::InstanceInfo;
use crate::local_runtime::LocalRuntime;
use crate::project::ProjectContext;
use crate::utils::{print_field, print_header, print_newline};
use eyre::Result;

pub async fn run() -> Result<()> {
    let project = match ProjectContext::find_and_load(None) {
        Ok(project) => project,
        Err(_) => {
            crate::utils::print_error("Not in a Helix project directory. Run 'helix init' first.");
            return Ok(());
        }
    };

    print_header("Helix Project Status");
    print_field("Project", &project.config.project.name);
    print_field("Root", &project.root.display().to_string());
    print_newline();

    let runtime = LocalRuntime::new(&project);
    print_header("Instances");
    for name in project.config.list_instances() {
        match project.config.get_instance(name)? {
            InstanceInfo::Local(config) => {
                let status = runtime.status(name)?;
                let state = status
                    .as_ref()
                    .map(|status| status.status.as_str())
                    .unwrap_or("not created");
                print_field(
                    &format!("{name} (local)"),
                    &format!("http://localhost:{} - {state}", config.port),
                );
            }
            InstanceInfo::Enterprise(config) => {
                let gateway = config
                    .gateway_url
                    .as_deref()
                    .unwrap_or("gateway not configured");
                print_field(
                    &format!("{name} (Enterprise)"),
                    &format!("cluster {} - {gateway}", config.cluster_id),
                );
            }
        }
    }

    Ok(())
}
