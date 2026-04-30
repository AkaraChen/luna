use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let web_dir = manifest_dir.join("../../apps/asahi-web");
    let dist_dir = web_dir.join("dist");

    println!("cargo:rerun-if-env-changed=ASAHI_SKIP_WEB_BUILD");
    register_web_inputs(&manifest_dir.join("../.."), &web_dir);

    if should_build_web() {
        build_web(&web_dir);
    } else {
        println!("cargo:warning=skipping asahi web build because ASAHI_SKIP_WEB_BUILD is set");
    }

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let generated_path = out_dir.join("embedded_web.rs");

    let mut files = Vec::new();
    if dist_dir.is_dir() {
        collect_files(&dist_dir, &dist_dir, &mut files);
    } else {
        println!(
            "cargo:warning=asahi web dist not found at {}; the embedded dashboard will use a fallback page",
            dist_dir.display()
        );
    }

    files.sort_by(|left, right| left.0.cmp(&right.0));

    let mut output = String::new();
    output
        .push_str("pub fn embedded_asset(path: &str) -> Option<(&'static [u8], &'static str)> {\n");
    output.push_str("    match path {\n");
    for (relative, absolute) in &files {
        let content_type = content_type_for(relative);
        let relative_literal = rust_string(relative);
        let absolute_literal = rust_string(&absolute.to_string_lossy());
        output.push_str(&format!(
            "        {relative_literal} => Some((include_bytes!({absolute_literal}), {content_type:?})),\n"
        ));
    }
    output.push_str("        _ => None,\n");
    output.push_str("    }\n");
    output.push_str("}\n");
    output.push_str("pub fn embedded_index() -> Option<(&'static [u8], &'static str)> {\n");
    output.push_str("    embedded_asset(\"index.html\")\n");
    output.push_str("}\n");

    fs::write(generated_path, output).unwrap();
}

fn should_build_web() -> bool {
    std::env::var("ASAHI_SKIP_WEB_BUILD")
        .map(|value| !matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(true)
}

fn build_web(web_dir: &Path) {
    if !web_dir.is_dir() {
        println!(
            "cargo:warning=asahi web directory not found at {}; skipping web build",
            web_dir.display()
        );
        return;
    }

    let status = Command::new("bun")
        .arg("run")
        .arg("build")
        .current_dir(web_dir)
        .status()
        .unwrap_or_else(|err| {
            panic!(
                "failed to run `bun run build` in {}: {err}. Install Bun or set ASAHI_SKIP_WEB_BUILD=1 to compile with the fallback page.",
                web_dir.display()
            )
        });

    if !status.success() {
        panic!(
            "`bun run build` failed in {} with status {status}. Fix the web build or set ASAHI_SKIP_WEB_BUILD=1 to compile with the fallback page.",
            web_dir.display()
        );
    }
}

fn register_web_inputs(workspace_dir: &Path, web_dir: &Path) {
    rerun_if_changed(&workspace_dir.join("bun.lock"));

    for file in [
        "package.json",
        "components.json",
        "index.html",
        "tsconfig.json",
        "vite.config.ts",
    ] {
        rerun_if_changed(&web_dir.join(file));
    }
    register_tree(&web_dir.join("src"));
}

fn register_tree(path: &Path) {
    if !path.exists() {
        return;
    }
    rerun_if_changed(path);

    let Ok(entries) = fs::read_dir(path) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            register_tree(&path);
        } else if path.is_file() {
            rerun_if_changed(&path);
        }
    }
}

fn rerun_if_changed(path: &Path) {
    if path.exists() {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}

fn collect_files(root: &Path, dir: &Path, files: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, files);
        } else if path.is_file() {
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            files.push((relative, path));
        }
    }
}

fn content_type_for(path: &str) -> &'static str {
    match Path::new(path).extension().and_then(|ext| ext.to_str()) {
        Some("css") => "text/css; charset=utf-8",
        Some("html") => "text/html; charset=utf-8",
        Some("ico") => "image/x-icon",
        Some("js") => "text/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("map") => "application/json; charset=utf-8",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("txt") => "text/plain; charset=utf-8",
        Some("webp") => "image/webp",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

fn rust_string(value: &str) -> String {
    format!("{value:?}")
}
