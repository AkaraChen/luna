use clash_brush_parser::ast::{
    AndOr, Command, CommandPrefixOrSuffixItem, CompoundCommand, CompoundList, CompoundListItem,
    Pipeline, SimpleCommand,
};
use clash_brush_parser::{ast, Parser, ParserOptions};
use std::io::Cursor;

/// Check if any command in the shell string matches any of the given patterns.
/// Patterns are space-separated tokens, e.g. "git commit" matches the command
/// `git` with first argument `commit`.
pub fn matches_shell_activity_pattern(command: &str, patterns: &[String]) -> bool {
    matches!(
        inspect_shell_activity(command, patterns),
        ShellActivityInspection::Matched(_)
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellActivityMatch {
    pub pattern: String,
    pub command_tokens: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShellActivityInspection {
    Matched(ShellActivityMatch),
    NoMatch { parsed_commands: Vec<Vec<String>> },
    ParseFailed,
    NoPatterns,
}

pub fn inspect_shell_activity(command: &str, patterns: &[String]) -> ShellActivityInspection {
    if patterns.is_empty() {
        return ShellActivityInspection::NoPatterns;
    }

    let commands = match parse_shell_commands(command) {
        Some(cmds) => cmds,
        None => return ShellActivityInspection::ParseFailed,
    };

    for parsed in &commands {
        for pattern in patterns {
            if matches_pattern_tokens(parsed, pattern) {
                return ShellActivityInspection::Matched(ShellActivityMatch {
                    pattern: pattern.clone(),
                    command_tokens: parsed.clone(),
                });
            }
        }
    }

    ShellActivityInspection::NoMatch {
        parsed_commands: commands,
    }
}

fn parse_shell_commands(input: &str) -> Option<Vec<Vec<String>>> {
    let cursor = Cursor::new(input);
    let mut parser = Parser::new(cursor, &ParserOptions::default());
    let program = parser.parse_program().ok()?;
    let mut commands = Vec::new();

    for complete_cmd in &program.complete_commands {
        extract_from_compound_list(&mut commands, complete_cmd);
    }

    // Handle shell interpreters with -c argument (bash -c "...", sh -c "...")
    let mut extra_commands = Vec::new();
    for cmd in &commands {
        if let Some(first) = cmd.first() {
            if is_shell_interpreter(first) {
                if let Some(c_pos) = cmd.iter().position(|arg| arg == "-c") {
                    if let Some(script) = cmd.get(c_pos + 1) {
                        let script = unquote_str(script);
                        if let Some(nested) = parse_shell_commands(&script) {
                            extra_commands.extend(nested);
                        }
                    }
                }
            }
        }
    }
    commands.extend(extra_commands);

    Some(commands)
}

fn is_shell_interpreter(name: &str) -> bool {
    matches!(
        name.to_lowercase().as_str(),
        "bash" | "sh" | "zsh" | "dash" | "ksh" | "fish"
    )
}

/// Remove a single layer of outer quotes if present.
fn unquote_str(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 {
        let bytes = s.as_bytes();
        let first = bytes[0] as char;
        let last = bytes[bytes.len() - 1] as char;
        if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

fn matches_pattern_tokens(tokens: &[String], pattern: &str) -> bool {
    let pattern_tokens: Vec<&str> = pattern.split_whitespace().collect();
    if pattern_tokens.is_empty() {
        return false;
    }
    if tokens.len() < pattern_tokens.len() {
        return false;
    }
    for (i, pt) in pattern_tokens.iter().enumerate() {
        if !tokens[i].eq_ignore_ascii_case(pt) {
            return false;
        }
    }
    true
}

fn extract_from_compound_list(commands: &mut Vec<Vec<String>>, list: &CompoundList) {
    for item in &list.0 {
        extract_from_compound_list_item(commands, item);
    }
}

fn extract_from_compound_list_item(commands: &mut Vec<Vec<String>>, item: &CompoundListItem) {
    extract_from_and_or_list(commands, &item.0);
}

fn extract_from_and_or_list(commands: &mut Vec<Vec<String>>, list: &ast::AndOrList) {
    extract_from_pipeline(commands, &list.first);
    for and_or in &list.additional {
        match and_or {
            AndOr::And(p) | AndOr::Or(p) => {
                extract_from_pipeline(commands, p);
            }
        }
    }
}

fn extract_from_pipeline(commands: &mut Vec<Vec<String>>, pipeline: &Pipeline) {
    for cmd in &pipeline.seq {
        extract_from_command(commands, cmd);
    }
}

fn extract_from_command(commands: &mut Vec<Vec<String>>, cmd: &Command) {
    match cmd {
        Command::Simple(simple) => {
            if let Some(tokens) = extract_from_simple_command(simple) {
                commands.push(tokens);
            }
        }
        Command::Compound(compound, _) => {
            extract_from_compound_command(commands, compound);
        }
        _ => {}
    }
}

fn extract_from_simple_command(simple: &SimpleCommand) -> Option<Vec<String>> {
    let mut tokens = Vec::new();
    if let Some(name) = &simple.word_or_name {
        tokens.push(name.value.clone());
    }
    if let Some(suffix) = &simple.suffix {
        for item in &suffix.0 {
            if let CommandPrefixOrSuffixItem::Word(w) = item {
                tokens.push(w.value.clone());
            }
        }
    }
    if tokens.is_empty() {
        None
    } else {
        Some(tokens)
    }
}

fn extract_from_compound_command(commands: &mut Vec<Vec<String>>, compound: &CompoundCommand) {
    match compound {
        CompoundCommand::Subshell(subshell) => {
            extract_from_compound_list(commands, &subshell.list);
        }
        CompoundCommand::BraceGroup(brace) => {
            extract_from_compound_list(commands, &brace.list);
        }
        CompoundCommand::IfClause(if_clause) => {
            extract_from_compound_list(commands, &if_clause.condition);
            extract_from_compound_list(commands, &if_clause.then);
            if let Some(elses) = &if_clause.elses {
                for else_clause in elses {
                    if let Some(condition) = &else_clause.condition {
                        extract_from_compound_list(commands, condition);
                    }
                    extract_from_compound_list(commands, &else_clause.body);
                }
            }
        }
        CompoundCommand::ForClause(for_clause) => {
            extract_from_compound_list(commands, &for_clause.body.list);
        }
        CompoundCommand::WhileClause(while_clause) | CompoundCommand::UntilClause(while_clause) => {
            extract_from_compound_list(commands, &while_clause.0);
            extract_from_compound_list(commands, &while_clause.1.list);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspects_matched_command() {
        assert_eq!(
            inspect_shell_activity("gh pr create --fill", &["gh pr create".to_string()]),
            ShellActivityInspection::Matched(ShellActivityMatch {
                pattern: "gh pr create".to_string(),
                command_tokens: vec![
                    "gh".to_string(),
                    "pr".to_string(),
                    "create".to_string(),
                    "--fill".to_string(),
                ],
            })
        );
    }

    #[test]
    fn inspects_no_match_with_parsed_commands() {
        assert_eq!(
            inspect_shell_activity("git status", &["git commit".to_string()]),
            ShellActivityInspection::NoMatch {
                parsed_commands: vec![vec!["git".to_string(), "status".to_string()]],
            }
        );
    }

    #[test]
    fn matches_simple_command() {
        assert!(matches_shell_activity_pattern(
            "git commit -m foo",
            &["git commit".to_string()]
        ));
    }

    #[test]
    fn matches_pipeline() {
        assert!(matches_shell_activity_pattern(
            "echo hello | gh pr create -F -",
            &["gh pr create".to_string()]
        ));
    }

    #[test]
    fn matches_andor_list() {
        assert!(matches_shell_activity_pattern(
            "git add . && git commit -m foo",
            &["git commit".to_string()]
        ));
    }

    #[test]
    fn matches_in_subshell() {
        assert!(matches_shell_activity_pattern(
            "bash -c \"git commit -m foo\"",
            &["git commit".to_string()]
        ));
    }

    #[test]
    fn matches_in_brace_group() {
        assert!(matches_shell_activity_pattern(
            "{ git add .; git commit; }",
            &["git commit".to_string()]
        ));
    }

    #[test]
    fn does_not_match_string_argument() {
        assert!(!matches_shell_activity_pattern(
            "echo \"git commit\"",
            &["git commit".to_string()]
        ));
    }

    #[test]
    fn matches_env_var_prefix() {
        assert!(matches_shell_activity_pattern(
            "GIT_PAGER=cat git log && git commit",
            &["git commit".to_string()]
        ));
    }

    #[test]
    fn matches_in_if_statement() {
        assert!(matches_shell_activity_pattern(
            "if git diff --quiet; then git commit -m foo; fi",
            &["git commit".to_string()]
        ));
    }

    #[test]
    fn matches_gh_pr_create() {
        assert!(matches_shell_activity_pattern(
            "gh pr create --title foo --body bar",
            &["gh pr create".to_string()]
        ));
    }
}
