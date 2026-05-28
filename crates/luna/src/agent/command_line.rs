use crate::error::{LunaError, Result};

/// Split a runner command into a program plus arguments, then append explicit
/// config args. This intentionally supports only the shell quoting Luna needs
/// for executable paths and simple default commands; it does not expand vars,
/// globs, pipes, redirects, or command substitutions.
pub fn split_command(command: &str, explicit_args: &[String]) -> Result<(String, Vec<String>)> {
    let mut parts = parse_command_words(command)?;
    if parts.is_empty() {
        return Err(LunaError::InvalidConfig(
            "runner command must be non-empty".to_string(),
        ));
    }

    let program = parts.remove(0);
    parts.extend(explicit_args.iter().cloned());
    Ok((program, parts))
}

fn parse_command_words(input: &str) -> Result<Vec<String>> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut started = false;

    for ch in input.chars() {
        if escaped {
            current.push(ch);
            started = true;
            escaped = false;
            continue;
        }

        if ch == '\\' {
            escaped = true;
            started = true;
            continue;
        }

        match quote {
            Some(q) if ch == q => {
                quote = None;
                started = true;
            }
            Some(_) => {
                current.push(ch);
                started = true;
            }
            None if ch == '\'' || ch == '"' => {
                quote = Some(ch);
                started = true;
            }
            None if ch.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            None => {
                current.push(ch);
                started = true;
            }
        }
    }

    if escaped {
        current.push('\\');
    }
    if let Some(q) = quote {
        return Err(LunaError::InvalidConfig(format!(
            "runner command has unterminated {q} quote"
        )));
    }
    if started {
        words.push(current);
    }

    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_command_and_appends_explicit_args() {
        let (program, args) =
            split_command("codex app-server", &["--experimental".to_string()]).unwrap();

        assert_eq!(program, "codex");
        assert_eq!(args, vec!["app-server", "--experimental"]);
    }

    #[test]
    fn keeps_quoted_executable_path_together() {
        let (program, args) =
            split_command("'/Applications/My Tools/agent' --foo \"bar baz\"", &[]).unwrap();

        assert_eq!(program, "/Applications/My Tools/agent");
        assert_eq!(args, vec!["--foo", "bar baz"]);
    }
}
