use eyre::Result;
use self_update::cargo_crate_version;

use crate::output::{Operation, Step, Verbosity};
use crate::utils::print_error_with_hint;

const V1_TARGET_VERSION: &str = "2.3.5";
const V1_TARGET_TAG: &str = "v2.3.5";

pub async fn run(force: bool, v1: bool) -> Result<()> {
    // We're using the self_update crate which is very handy but doesn't support async.
    // Still, this is good enough, but because it panics in an async context we must
    // do a spawn_blocking
    tokio::task::spawn_blocking(move || run_sync(force, v1)).await?
}

fn run_sync(force: bool, v1: bool) -> Result<()> {
    let op = Operation::new("Updating", "CLI");

    let mut check_step = Step::with_messages("Checking for updates", "Checked for updates");
    check_step.start();

    let mut update_builder = self_update::backends::github::Update::configure();
    update_builder
        .repo_owner("HelixDB")
        .repo_name("helix-db")
        .bin_name("helix")
        .show_download_progress(true)
        .show_output(false)
        .no_confirm(true)
        .current_version(cargo_crate_version!());

    if v1 {
        update_builder.target_version_tag(V1_TARGET_TAG);
    }

    let status = update_builder.build()?;

    let current_version = cargo_crate_version!();

    if !force {
        let target_release = if v1 {
            status.get_release_version(V1_TARGET_TAG)?
        } else {
            status.get_latest_release()?
        };

        if target_release.version == current_version {
            check_step.done_with_info("already up to date");
            op.success();
            println!("  Use --force to reinstall");
            return Ok(());
        }

        check_step.done_with_info(&format!(
            "v{current_version} -> v{}",
            target_release.version
        ));
    } else if v1 {
        check_step.done_with_info(&format!("force update to v{V1_TARGET_VERSION}"));
    } else {
        check_step.done_with_info("force update");
    }

    let mut install_step =
        Step::with_messages("Downloading and installing", "Downloaded and installed");
    install_step.start();

    match status.update() {
        Ok(_) => {
            install_step.done();
            op.success();
            if Verbosity::current().show_normal() {
                Operation::print_details(&[(
                    "Note",
                    "Please restart your terminal to use the new version",
                )]);
            }
        }
        Err(e) => {
            install_step.fail();
            op.failure();
            print_error_with_hint(
                &format!("Update failed: {e}"),
                "check your internet connection and try again",
            );
            return Err(e.into());
        }
    }

    Ok(())
}
