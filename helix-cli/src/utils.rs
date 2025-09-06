use eyre::Result;
use std::fs;
use std::path::Path;

/// Copy a directory recursively
pub fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    if !src.is_dir() {
        return Err(eyre::eyre!("Source is not a directory: {}", src.display()));
    }
    
    // Create destination directory
    fs::create_dir_all(dst)?;
    
    // Read the source directory
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    
    Ok(())
}

/// Check if a command exists in PATH
pub fn command_exists(command: &str) -> bool {
    std::process::Command::new("which")
        .arg(command)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Print a status message with a prefix
pub fn print_status(prefix: &str, message: &str) {
    println!("[{}] {}", prefix, message);
}

/// Print an error message
pub fn print_error(message: &str) {
    eprintln!("[ERROR] {}", message);
}

/// Print a success message
pub fn print_success(message: &str) {
    println!("[SUCCESS] {}", message);
}

/// Print a warning message
pub fn print_warning(message: &str) {
    println!("[WARNING] {}", message);
}