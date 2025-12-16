//! Send feedback to the Helix team via GitHub issues.

use crate::prompts;
use crate::utils::{print_info, print_success};
use eyre::{eyre, Result};

const GITHUB_ISSUE_URL: &str = "https://github.com/helixdb/helix-db/issues/new";
const MAX_URL_LENGTH: usize = 8000;

/// Type of feedback being submitted
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackType {
    Bug,
    FeatureRequest,
    General,
}

impl FeedbackType {
    /// Get the GitHub issue type parameter
    fn issue_type(&self) -> &'static str {
        match self {
            FeedbackType::Bug => "Bug",
            FeedbackType::FeatureRequest => "Feature",
            FeedbackType::General => "Feature",
        }
    }

    /// Get the GitHub labels for this feedback type
    fn labels(&self) -> &'static str {
        match self {
            FeedbackType::Bug => "bug,cli",
            FeedbackType::FeatureRequest => "enhancement",
            FeedbackType::General => "feedback",
        }
    }

    /// Get a title prefix for the issue
    fn title_prefix(&self) -> &'static str {
        match self {
            FeedbackType::Bug => "bug: ",
            FeedbackType::FeatureRequest => "feature: ",
            FeedbackType::General => "feedback: ",
        }
    }
}

/// Run the feedback command
pub async fn run(message: Option<String>) -> Result<()> {
    let (feedback_type, feedback_message) = if let Some(msg) = message {
        // Inline message provided - default to General feedback
        (FeedbackType::General, msg)
    } else {
        // Interactive mode
        if !prompts::is_interactive() {
            return Err(eyre!(
                "No feedback message provided. Run 'helix feedback \"your message\"' or run in an interactive terminal."
            ));
        }

        prompts::intro("helix feedback")?;
        let feedback_type = prompts::select_feedback_type()?;
        let feedback_message = prompts::input_feedback_message()?;

        if !prompts::confirm("Open browser to submit feedback?")? {
            print_info("Feedback cancelled.");
            return Ok(());
        }

        (feedback_type, feedback_message)
    };

    // Build and open the GitHub issue URL
    let url = build_issue_url(feedback_type, &feedback_message);
    print_info("Opening browser to submit feedback...");

    open::that(&url).map_err(|e| eyre!("Failed to open browser: {}", e))?;

    print_success("Browser opened! Complete your feedback submission on GitHub.");
    Ok(())
}

/// Build the GitHub issue URL with pre-filled content
fn build_issue_url(feedback_type: FeedbackType, message: &str) -> String {
    let title = format!(
        "{}{}",
        feedback_type.title_prefix(),
        truncate_for_title(message)
    );

    let body = build_issue_body(message);

    let encoded_title = urlencoding::encode(&title);
    let encoded_body = urlencoding::encode(&body);
    let encoded_labels = urlencoding::encode(feedback_type.labels());
    let encoded_type = urlencoding::encode(feedback_type.issue_type());

    let url = format!(
        "{}?type={}&title={}&body={}&labels={}",
        GITHUB_ISSUE_URL, encoded_type, encoded_title, encoded_body, encoded_labels
    );

    // Truncate body if URL is too long
    if url.len() > MAX_URL_LENGTH {
        let truncated_body = build_truncated_body(message);
        let encoded_body = urlencoding::encode(&truncated_body);
        format!(
            "{}?type={}&title={}&body={}&labels={}",
            GITHUB_ISSUE_URL, encoded_type, encoded_title, encoded_body, encoded_labels
        )
    } else {
        url
    }
}

/// Build the full issue body
fn build_issue_body(message: &str) -> String {
    let mut body = String::new();

    // Environment section
    body.push_str("## Environment\n");
    body.push_str(&format!(
        "- Helix CLI version: {}\n",
        env!("CARGO_PKG_VERSION")
    ));
    body.push_str(&format!("- OS: {}\n\n", std::env::consts::OS));

    // Feedback section
    body.push_str("## Feedback\n");
    body.push_str(message);

    body
}

/// Build a truncated body for when the URL would be too long
fn build_truncated_body(message: &str) -> String {
    let mut body = String::new();

    // Environment section (always include)
    body.push_str("## Environment\n");
    body.push_str(&format!(
        "- Helix CLI version: {}\n",
        env!("CARGO_PKG_VERSION")
    ));
    body.push_str(&format!("- OS: {}\n\n", std::env::consts::OS));

    // Feedback section (truncated)
    body.push_str("## Feedback\n");
    let truncated: String = message.chars().take(3000).collect();
    body.push_str(&truncated);
    if message.len() > 3000 {
        body.push_str("\n\n_[Message truncated due to URL length limits]_");
    }

    body
}

/// Truncate message to create a reasonable issue title
fn truncate_for_title(message: &str) -> String {
    let first_line = message.lines().next().unwrap_or(message);
    if first_line.len() > 50 {
        format!("{}...", &first_line[..47])
    } else {
        first_line.to_string()
    }
}

/// URL encoding utilities
mod urlencoding {
    pub fn encode(input: &str) -> String {
        let mut encoded = String::new();
        for byte in input.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    encoded.push(byte as char);
                }
                b' ' => encoded.push_str("%20"),
                _ => {
                    encoded.push_str(&format!("%{:02X}", byte));
                }
            }
        }
        encoded
    }
}
