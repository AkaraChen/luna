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
