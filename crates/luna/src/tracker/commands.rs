use crate::{error::Result, model::Issue};

use super::context::{TrackerTargetOptions, resolve_tracker_issue};

#[derive(Debug, Clone)]
pub struct CommentCommandOptions {
    pub target: TrackerTargetOptions,
    pub body: String,
}

#[derive(Debug, Clone)]
pub struct ShowCommandOptions {
    pub target: TrackerTargetOptions,
    pub json: bool,
}

#[derive(Debug, Clone)]
pub struct MoveCommandOptions {
    pub target: TrackerTargetOptions,
    pub state: String,
}

pub async fn run_comment_command(options: CommentCommandOptions) -> Result<String> {
    let (tracker, issue) = resolve_tracker_issue(&options.target).await?;
    tracker.create_comment(&issue, &options.body).await?;
    Ok(issue.identifier)
}

pub async fn run_show_command(options: ShowCommandOptions) -> Result<String> {
    let (_tracker, issue) = resolve_tracker_issue(&options.target).await?;
    if options.json {
        Ok(serde_json::to_string_pretty(&issue)?)
    } else {
        Ok(format_issue(&issue))
    }
}

pub async fn run_move_command(options: MoveCommandOptions) -> Result<String> {
    let (tracker, issue) = resolve_tracker_issue(&options.target).await?;
    tracker
        .update_issue_state(&issue.id, &options.state)
        .await?;
    Ok(issue.identifier)
}

fn format_issue(issue: &Issue) -> String {
    let mut output = Vec::new();
    output.push(format!("Issue: {}", issue.identifier));
    output.push(format!("Title: {}", issue.title));
    output.push(format!("State: {}", issue.state));

    if let Some(priority) = issue.priority {
        output.push(format!("Priority: {priority}"));
    }
    if let Some(url) = issue.url.as_deref() {
        output.push(format!("URL: {url}"));
    }
    if let Some(branch_name) = issue.branch_name.as_deref() {
        output.push(format!("Branch: {branch_name}"));
    }
    if !issue.labels.is_empty() {
        output.push(format!("Labels: {}", issue.labels.join(", ")));
    }
    if !issue.blocked_by.is_empty() {
        let blocked_by = issue
            .blocked_by
            .iter()
            .map(|blocker| {
                blocker
                    .identifier
                    .clone()
                    .or_else(|| blocker.id.clone())
                    .unwrap_or_else(|| "unknown".to_string())
            })
            .collect::<Vec<_>>()
            .join(", ");
        output.push(format!("Blocked by: {blocked_by}"));
    }
    if let Some(description) = issue.description.as_deref() {
        output.push(String::new());
        output.push("Description:".to_string());
        output.push(description.to_string());
    }

    output.join("\n")
}

#[cfg(test)]
mod tests {
    use crate::model::Issue;

    use super::format_issue;

    #[test]
    fn formats_issue_summary() {
        let issue = Issue {
            id: "id".to_string(),
            identifier: "ENG-42".to_string(),
            title: "Fix tracker CLI".to_string(),
            description: Some("Detailed description".to_string()),
            priority: Some(1),
            state: "In Progress".to_string(),
            branch_name: Some("eng-42".to_string()),
            url: Some("https://example.com".to_string()),
            labels: vec!["backend".to_string(), "cli".to_string()],
            blocked_by: Vec::new(),
            created_at: None,
            updated_at: None,
        };

        let text = format_issue(&issue);
        assert!(text.contains("Issue: ENG-42"));
        assert!(text.contains("Priority: 1"));
        assert!(text.contains("Description:"));
    }
}
