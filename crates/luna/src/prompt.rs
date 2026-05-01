use minijinja::{Environment, UndefinedBehavior, context};

use crate::{
    error::{LunaError, Result},
    model::Issue,
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

pub fn build_continuation_prompt(issue: &Issue, turn_number: u32, max_turns: u32) -> String {
    format!(
        "Continue working on issue {}: {}. \
The original task is already in thread history. Re-check the workspace, continue from the current state, \
and stop only at a natural handoff point. This is continuation turn {turn_number}/{max_turns}.",
        issue.identifier, issue.title
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::model::{Issue, ProjectRef};

    use super::render_issue_prompt;

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
