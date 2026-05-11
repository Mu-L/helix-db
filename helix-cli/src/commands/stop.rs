use crate::config::InstanceInfo;
use crate::local_runtime::LocalRuntime;
use crate::output::Operation;
use crate::project::ProjectContext;
use eyre::{Result, eyre};

pub async fn run(instance: Option<String>) -> Result<()> {
    let project = ProjectContext::find_and_load(None)?;
    let instance = instance.unwrap_or_else(|| "dev".to_string());
    if !matches!(
        project.config.get_instance(&instance)?,
        InstanceInfo::Local(_)
    ) {
        return Err(eyre!("'{instance}' is not a local v2 instance"));
    }
    let op = Operation::new("Stopping", &instance);
    LocalRuntime::new(&project).stop(&instance)?;
    op.success();
    Ok(())
}
