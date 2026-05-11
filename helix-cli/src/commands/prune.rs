use crate::config::ContainerRuntime;
use crate::local_runtime::LocalRuntime;
use crate::output::Operation;
use crate::project::ProjectContext;
use crate::utils::{print_confirm, print_warning};
use eyre::Result;

pub async fn run(instance: Option<String>, all: bool) -> Result<()> {
    let project = ProjectContext::find_and_load(None)?;
    if all {
        prune_all(&project).await
    } else if let Some(instance) = instance {
        prune_one(&project, &instance).await
    } else {
        prune_unused(&project).await
    }
}

async fn prune_one(project: &ProjectContext, instance: &str) -> Result<()> {
    let op = Operation::new("Pruning", instance);
    LocalRuntime::new(project).prune_instance(instance)?;
    let workspace = project.instance_workspace(instance);
    if workspace.exists() {
        std::fs::remove_dir_all(workspace)?;
    }
    op.success();
    Ok(())
}

async fn prune_all(project: &ProjectContext) -> Result<()> {
    print_warning("This will remove local v2 containers and workspaces for all local instances.");
    if !print_confirm("Continue?")? {
        return Ok(());
    }
    for instance in project.config.local.keys() {
        prune_one(project, instance).await?;
    }
    Ok(())
}

async fn prune_unused(project: &ProjectContext) -> Result<()> {
    let op = Operation::new("Pruning", "unused local runtime resources");
    LocalRuntime::check_available(project.config.project.container_runtime)?;
    let runtime = LocalRuntime::new(project);
    let output = runtime.run_command(&["system", "prune", "-f"])?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(eyre::eyre!(
            "Failed to prune {} resources:\n{stderr}",
            project.config.project.container_runtime.binary()
        ));
    }
    op.success();
    Ok(())
}

#[allow(dead_code)]
fn _runtime_label(runtime: ContainerRuntime) -> &'static str {
    runtime.label()
}
