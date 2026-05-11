use crate::config::InstanceInfo;
use crate::local_runtime::LocalRuntime;
use crate::output::Operation;
use crate::project::ProjectContext;
use eyre::{Result, eyre};

pub async fn run(instance: Option<String>) -> Result<()> {
    let project = ProjectContext::find_and_load(None)?;
    let instance = instance.unwrap_or_else(|| "dev".to_string());
    let InstanceInfo::Local(config) = project.config.get_instance(&instance)? else {
        return Err(eyre!("'{instance}' is not a local v2 instance"));
    };
    let op = Operation::new("Restarting", &instance);
    LocalRuntime::new(&project).restart(&instance, config)?;
    op.success();
    Ok(())
}
