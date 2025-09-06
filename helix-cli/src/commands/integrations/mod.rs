use async_trait::async_trait;
use eyre::Result;
use crate::project::ProjectContext;

pub mod fly;

#[async_trait]
pub trait Integration {
    async fn init(&self, project: &ProjectContext, instance_name: &str) -> Result<()>;
    async fn deploy(&self, instance_name: &str) -> Result<()>;
    async fn start(&self, project: &ProjectContext, instance_name: &str) -> Result<()>;
    async fn stop(&self, project: &ProjectContext, instance_name: &str) -> Result<()>;
}
