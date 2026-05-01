use minijinja::{Environment, UndefinedBehavior, context};

use crate::{
    error::{LunaError, Result},
    model::{Comment, Issue},
};

const DEFAULT_PROMPT: &str = "You are working on an issue from Linear.";

pub fn render_issue_prompt(template: &str, issue: &Issue, attempt: Option<u32>) -> Result<String> {
    let mut env = Environment::new();
    env.set_undefined_behavior(UndefinedBehavior::Strict);

    let template = if template.trim().is_empty() {
        DEFAULT_PROMPT
    } else {
        template
    };

    env.add_template("workflow", template)
        .map_err(|err| LunaError::TemplateParseError(err.to_string()))?;

    let compiled = env
        .get_template("workflow")
        .map_err(|err| LunaError::TemplateParseError(err.to_string()))?;

    compiled
        .render(context! {
            issue => issue,
            attempt => attempt,
        })
        .map_err(|err| LunaError::TemplateRenderError(err.to_string()))
}

pub fn build_continuation_prompt(
    issue: &Issue,
    turn_number: u32,
    max_turns: u32,
    new_comments: &[Comment],
) -> String {
    let mut prompt = format!(
        "Continue working on issue {}: {}. \
The original task is already in thread history. Re-check the workspace, continue from the current state, \
and stop only at a natural handoff point. This is continuation turn {turn_number}/{max_turns}.",
        issue.identifier, issue.title
    );

    if !new_comments.is_empty() {
        let comments_text: String = new_comments
            .iter()
            .map(|c| format!("- {}", c.body.trim()))
            .collect::<Vec<_>>()
            .join("\n");
        prompt = format!(
            "{prompt}\n\nNew comments on this issue:\n{comments_text}"
        );
    }

    prompt
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::model::{Issue, ProjectRef};

    use super::{build_continuation_prompt, render_issue_prompt};

    fn test_issue() -> Issue {
        Issue {
            id: "1".to_string(),
            identifier: "TEST-1".to_string(),
            title: "Test".to_string(),
            description: None,
            priority: Some(1),
            state: "Todo".to_string(),
            branch_name: None,
            url: None,
            labels: vec![],
            blocked_by: vec![],
            created_at: None,
            updated_at: None,
            project: None,
            source_data: None,
        }
    }

    #[test]
    fn continuation_prompt_without_comments() {
        let issue = test_issue();
        let prompt = build_continuation_prompt(&issue, 2, 5, &[]);
        assert!(prompt.contains("continuation turn 2/5"));
        assert!(!prompt.contains("New comments"));
    }

    #[test]
    fn continuation_prompt_with_comments() {
        let issue = test_issue();
        let comments = vec![
            crate::model::Comment {
                id: "c1".to_string(),
                issue_id: issue.id.clone(),
                body: "Please fix the typo".to_string(),
                created_at: chrono::Utc::now(),
            },
            crate::model::Comment {
                id: "c2".to_string(),
                issue_id: issue.id.clone(),
                body: "Also update the docs".to_string(),
                created_at: chrono::Utc::now(),
            },
        ];
        let prompt = build_continuation_prompt(&issue, 2, 5, &comments);
        assert!(prompt.contains("continuation turn 2/5"));
        assert!(prompt.contains("New comments on this issue:"));
        assert!(prompt.contains("- Please fix the typo"));
        assert!(prompt.contains("- Also update the docs"));
    }

    #[test]
    fn renders_issue_with_source_data() {
        let issue = Issue {
            id: "1".to_string(),
            identifier: "TEST-1".to_string(),
            title: "Test".to_string(),
            description: None,
            priority: Some(1),
            state: "Todo".to_string(),
            branch_name: None,
            url: None,
            labels: vec![],
            blocked_by: vec![],
            created_at: None,
            updated_at: None,
            project: None,
            source_data: Some(json!({"field": "value"})),
        };

        let template = "Source: {{ issue.source_data.field }}";
        let result = render_issue_prompt(template, &issue, None).unwrap();
        assert_eq!(result, "Source: value");
    }

    #[test]
    fn renders_issue_with_project() {
        let issue = Issue {
            id: "1".to_string(),
            identifier: "TEST-1".to_string(),
            title: "Test".to_string(),
            description: None,
            priority: Some(1),
            state: "Todo".to_string(),
            branch_name: None,
            url: None,
            labels: vec![],
            blocked_by: vec![],
            created_at: None,
            updated_at: None,
            project: Some(ProjectRef {
                id: "p1".to_string(),
                slug: "proj".to_string(),
                name: "Project".to_string(),
                state: "Active".to_string(),
                priority: Some(1),
            }),
            source_data: None,
        };

        let template = "Project: {{ issue.project.name }}";
        let result = render_issue_prompt(template, &issue, None).unwrap();
        assert_eq!(result, "Project: Project");
    }
}
