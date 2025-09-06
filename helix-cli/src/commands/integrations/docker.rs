use eyre::Result;
use tokio::process::Command;

pub struct DockerManager {}

impl DockerManager {
    pub async fn tag(image_name: &str, image_tag: &str, registry_url: &str) -> Result<()> {
        let local_image = format!("{image_name}:{image_tag}");
        let registry_image = format!("{registry_url}/{image_name}:{image_tag}");
        Command::new("docker")
            .arg("tag")
            .arg(&local_image)
            .arg(&registry_image)
            .spawn()?; // TODO: Wait?
        Ok(())
    }

    pub async fn push(image_name: &str, image_tag: &str, registry_url: &str) -> Result<()> {
        let registry_image = format!("{registry_url}/{image_name}:{image_tag}");
        Command::new("docker")
            .arg("push")
            .arg(&registry_image)
            .spawn()?; // TODO: Wait?
        // TODO: Check if pushed
        Ok(())
    }
}
