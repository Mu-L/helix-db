use crate::config::InstanceInfo;
use crate::local_runtime::LocalRuntime;
use crate::output::{Operation, Verbosity};
use crate::project::ProjectContext;
use eyre::{Result, eyre};

pub async fn run(instance: Option<String>, detach: bool, port: Option<u16>) -> Result<()> {
    let project = ProjectContext::find_and_load(None)?;
    let instance = resolve_local_instance(&project, instance)?;
    let InstanceInfo::Local(config) = project.config.get_instance(&instance)? else {
        return Err(eyre!("'{instance}' is not a local v2 instance"));
    };
    let mut config = config.clone();
    if let Some(port) = port {
        config.port = port;
    }

    let op = Operation::new("Running", &instance);
    crate::output::warning(
        "Local enterprise-dev uses in-memory storage. Stopping or restarting wipes local data.",
    );

    project.ensure_instance_dir(&instance)?;
    let runtime = LocalRuntime::new(&project);
    if detach {
        runtime.run_detached(&instance, &config)?;
        op.success();
        if Verbosity::current().show_normal() {
            Operation::print_details(&[
                ("URL", &format!("http://localhost:{}", config.port)),
                ("Container", &runtime.container_name(&instance)),
            ]);
        }
    } else {
        crate::output::info("Running in foreground. Press Ctrl-C to stop.");
        runtime.run_foreground(&instance, &config)?;
        op.success();
    }

    Ok(())
}

fn resolve_local_instance(project: &ProjectContext, instance: Option<String>) -> Result<String> {
    if let Some(instance) = instance {
        return Ok(instance);
    }
    if project.config.local.contains_key("dev") {
        return Ok("dev".to_string());
    }
    if project.config.local.len() == 1 {
        return Ok(project.config.local.keys().next().unwrap().clone());
    }
    Err(eyre!("No local instance specified"))
}
