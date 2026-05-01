use std::{collections::HashMap, path::Path};

use asahi::domain::{WikiNode, WikiNodeKind};
use bashkit::{FileSystem, InMemoryFs};

use crate::error::Result;

/// Build a bashkit [`InMemoryFs`] populated with wiki nodes.
///
/// Folders become directories and pages become files. Paths are built from
/// each node's slug and its ancestor chain.
pub async fn build_wiki_fs(nodes: Vec<WikiNode>) -> Result<InMemoryFs> {
    let fs = InMemoryFs::new();
    let by_id: HashMap<String, &WikiNode> = nodes.iter().map(|n| (n.id.clone(), n)).collect();

    // 1. Create all folders first (recursive mkdir so order doesn't matter)
    for node in &nodes {
        if node.kind == WikiNodeKind::Folder {
            let path = compute_path(node, &by_id);
            fs.mkdir(Path::new(&path), true).await?;
        }
    }

    // 2. Write all pages
    for node in &nodes {
        if node.kind == WikiNodeKind::Page {
            let path = compute_path(node, &by_id);
            let html = node.content.clone().unwrap_or_default();
            let markdown = html2md::parse_html(&html);
            fs.write_file(Path::new(&path), markdown.as_bytes()).await?;
        }
    }

    Ok(fs)
}

/// Compute the virtual filesystem path for a wiki node.
///
/// Folders are mounted at `/slug/` but we return `/slug` so that bashkit
/// mkdir creates the directory. Pages are mounted at `/ancestor/slug`.
fn compute_path(node: &WikiNode, by_id: &HashMap<String, &WikiNode>) -> String {
    let mut segments = vec![node.slug.clone()];
    let mut current = node;

    while let Some(parent_id) = current.parent_id.as_deref() {
        if let Some(parent) = by_id.get(parent_id) {
            segments.push(parent.slug.clone());
            current = parent;
        } else {
            break;
        }
    }

    segments.reverse();
    let path = format!("/{}", segments.join("/"));

    if node.kind == WikiNodeKind::Folder {
        // Ensure folder paths end with a slash for consistency,
        // but bashkit mkdir works with or without it.
        format!("{}/", path)
    } else {
        format!("{}.md", path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asahi::domain::{WikiNode, WikiNodeKind};

    fn folder(id: &str, parent_id: Option<&str>, slug: &str) -> WikiNode {
        WikiNode {
            id: id.to_string(),
            project_id: "proj".to_string(),
            parent_id: parent_id.map(|s| s.to_string()),
            kind: WikiNodeKind::Folder,
            title: slug.to_string(),
            slug: slug.to_string(),
            content: None,
            current_version: None,
            created_at: None,
            updated_at: None,
            deleted_at: None,
        }
    }

    fn page(id: &str, parent_id: Option<&str>, slug: &str, content: &str) -> WikiNode {
        WikiNode {
            id: id.to_string(),
            project_id: "proj".to_string(),
            parent_id: parent_id.map(|s| s.to_string()),
            kind: WikiNodeKind::Page,
            title: slug.to_string(),
            slug: slug.to_string(),
            content: Some(content.to_string()),
            current_version: None,
            created_at: None,
            updated_at: None,
            deleted_at: None,
        }
    }

    #[tokio::test]
    async fn builds_virtual_fs() {
        let nodes = vec![
            folder("f1", None, "guides"),
            folder("f2", Some("f1"), "backend"),
            page("p1", Some("f2"), "design", "<h1>Design</h1>"),
            page("p2", None, "readme", "<p>Hello</p>"),
        ];

        let fs = build_wiki_fs(nodes).await.unwrap();

        // Root page
        let readme = fs.read_file(Path::new("/readme.md")).await.unwrap();
        assert!(String::from_utf8(readme).unwrap().contains("Hello"));

        // Nested page
        let design = fs.read_file(Path::new("/guides/backend/design.md")).await.unwrap();
        assert!(String::from_utf8(design).unwrap().contains("Design"));

        // Directory listing
        let root_entries = fs.read_dir(Path::new("/")).await.unwrap();
        let names: Vec<String> = root_entries.into_iter().map(|e| e.name).collect();
        assert!(names.contains(&"guides".to_string()));
        assert!(names.contains(&"readme.md".to_string()));

        let backend_entries = fs.read_dir(Path::new("/guides/backend")).await.unwrap();
        let names: Vec<String> = backend_entries.into_iter().map(|e| e.name).collect();
        assert!(names.contains(&"design.md".to_string()));
    }
}
