use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use serde_yaml::{Mapping, Value as YamlValue};

use crate::{
    config::{ServiceConfig, resolve_service_config},
    error::{LunaError, Result},
    model::WorkflowDefinition,
};

#[derive(Clone, Debug)]
pub struct LoadedWorkflow {
    pub definition: WorkflowDefinition,
    pub config: ServiceConfig,
}

#[derive(Debug)]
pub struct WorkflowStore {
    path: PathBuf,
    modified_at: Option<SystemTime>,
    current: LoadedWorkflow,
}

impl WorkflowStore {
    pub fn load(path: PathBuf) -> Result<Self> {
        let (definition, modified_at) = load_definition(&path)?;
        let config = resolve_service_config(&definition, &path)?;
        Ok(Self {
            path,
            modified_at,
            current: LoadedWorkflow { definition, config },
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn current(&self) -> &LoadedWorkflow {
        &self.current
    }

    pub fn reload_if_changed(&mut self) -> Result<bool> {
        let modified_at = fs::metadata(&self.path)
            .map_err(|err| {
                if err.kind() == std::io::ErrorKind::NotFound {
                    LunaError::MissingWorkflowFile(self.path.clone())
                } else {
                    LunaError::Io(err)
                }
            })?
            .modified()
            .ok();

        if modified_at == self.modified_at {
            return Ok(false);
        }

        let (definition, modified_at) = load_definition(&self.path)?;
        let config = resolve_service_config(&definition, &self.path)?;
        self.modified_at = modified_at;
        self.current = LoadedWorkflow { definition, config };
        Ok(true)
    }
}

fn load_definition(path: &Path) -> Result<(WorkflowDefinition, Option<SystemTime>)> {
    let contents = fs::read_to_string(path).map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            LunaError::MissingWorkflowFile(path.to_path_buf())
        } else {
            LunaError::Io(err)
        }
    })?;
    let modified_at = fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok());
    let definition = parse_workflow_definition(&contents)?;
    Ok((definition, modified_at))
}

pub fn parse_workflow_definition(contents: &str) -> Result<WorkflowDefinition> {
    let normalized = contents.replace("\r\n", "\n");
    if !normalized.starts_with("---\n") && normalized.trim() != "---" {
        return Ok(WorkflowDefinition {
            config: Mapping::new(),
            prompt_template: normalized.trim().to_string(),
        });
    }

    let mut lines = normalized.lines();
    let first = lines.next().unwrap_or_default();
    if first != "---" {
        return Ok(WorkflowDefinition {
            config: Mapping::new(),
            prompt_template: normalized.trim().to_string(),
        });
    }

    let mut front_matter_lines = Vec::new();
    let mut found_end = false;
    for line in lines.by_ref() {
        if line == "---" {
            found_end = true;
            break;
        }
        front_matter_lines.push(line);
    }

    if !found_end {
        return Err(LunaError::WorkflowParseError(
            "front matter opened with --- but never closed".to_string(),
        ));
    }

    let body = lines.collect::<Vec<_>>().join("\n").trim().to_string();
    let front_matter = front_matter_lines.join("\n");
    let value: YamlValue = serde_yaml::from_str(&front_matter)?;
    let config = match value {
        YamlValue::Mapping(mapping) => mapping,
        _ => return Err(LunaError::WorkflowFrontMatterNotAMap),
    };

    Ok(WorkflowDefinition {
        config,
        prompt_template: body,
    })
}

#[cfg(test)]
mod tests {
    use super::parse_workflow_definition;

    #[test]
    fn parses_front_matter() {
        let workflow = parse_workflow_definition(
            "---\ntracker:\n  kind: github_project\n  owner: acme\n  project_number: 1\n---\nhello {{ issue.identifier }}\n",
        )
        .expect("workflow should parse");
        assert_eq!(workflow.prompt_template, "hello {{ issue.identifier }}");
        assert!(!workflow.config.is_empty());
    }

    #[test]
    fn parses_without_front_matter() {
        let workflow = parse_workflow_definition("body only").expect("workflow should parse");
        assert_eq!(workflow.prompt_template, "body only");
        assert!(workflow.config.is_empty());
    }
}
