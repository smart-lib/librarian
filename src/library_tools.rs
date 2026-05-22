use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::Config;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LibraryRoot {
    Library,
    Projects,
}

#[derive(Clone, Debug, Serialize)]
pub struct LibraryEntry {
    pub name: String,
    pub path: String,
    pub root: LibraryRoot,
    pub kind: LibraryEntryKind,
    pub children: Vec<LibraryEntry>,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LibraryEntryKind {
    Folder,
    Markdown,
    File,
}

#[derive(Clone, Debug, Serialize)]
pub struct ToolPath {
    pub root: LibraryRoot,
    pub path: String,
}

pub fn tree(config: &Config, root: LibraryRoot, max_depth: usize) -> Result<LibraryEntry> {
    let root_path = root_path(config, root);
    fs::create_dir_all(&root_path)
        .with_context(|| format!("Failed to create {}", root_path.display()))?;
    read_entry(root, &root_path, &root_path, max_depth.min(12))
}

pub fn create_folder(config: &Config, root: LibraryRoot, relative_path: &str) -> Result<ToolPath> {
    let path = resolve_new_path(config, root, relative_path)?;
    fs::create_dir_all(&path)
        .with_context(|| format!("Failed to create folder {}", path.display()))?;
    Ok(tool_path(config, root, &path))
}

pub fn create_empty_file(
    config: &Config,
    root: LibraryRoot,
    relative_path: &str,
) -> Result<ToolPath> {
    let path = resolve_new_path(config, root, relative_path)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create folder {}", parent.display()))?;
    }
    fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .with_context(|| format!("Failed to create file {}", path.display()))?;
    Ok(tool_path(config, root, &path))
}

pub fn read_markdown(config: &Config, relative_path: &str) -> Result<String> {
    let path = resolve_existing_path(config, LibraryRoot::Library, relative_path)?;
    ensure_markdown(&path)?;
    fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))
}

pub fn write_markdown(config: &Config, relative_path: &str, content: &str) -> Result<ToolPath> {
    let path = resolve_new_or_existing_path(config, LibraryRoot::Library, relative_path)?;
    ensure_markdown(&path)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create folder {}", parent.display()))?;
    }
    fs::write(&path, content).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(tool_path(config, LibraryRoot::Library, &path))
}

pub fn move_path(
    config: &Config,
    root: LibraryRoot,
    from_relative_path: &str,
    to_relative_path: &str,
) -> Result<ToolPath> {
    let from = resolve_existing_path(config, root, from_relative_path)?;
    let to = resolve_new_path(config, root, to_relative_path)?;
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create folder {}", parent.display()))?;
    }
    fs::rename(&from, &to)
        .with_context(|| format!("Failed to move {} to {}", from.display(), to.display()))?;
    Ok(tool_path(config, root, &to))
}

pub fn delete_path(
    config: &Config,
    root: LibraryRoot,
    relative_path: &str,
    recursive: bool,
) -> Result<ToolPath> {
    let path = resolve_existing_path(config, root, relative_path)?;
    let result = tool_path(config, root, &path);
    let metadata = fs::symlink_metadata(&path)
        .with_context(|| format!("Failed to inspect {}", path.display()))?;
    if metadata.is_dir() {
        if recursive {
            fs::remove_dir_all(&path)
                .with_context(|| format!("Failed to delete folder {}", path.display()))?;
        } else {
            fs::remove_dir(&path)
                .with_context(|| format!("Failed to delete empty folder {}", path.display()))?;
        }
    } else {
        fs::remove_file(&path)
            .with_context(|| format!("Failed to delete file {}", path.display()))?;
    }
    Ok(result)
}

fn read_entry(
    root: LibraryRoot,
    base: &Path,
    path: &Path,
    remaining_depth: usize,
) -> Result<LibraryEntry> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("Failed to inspect {}", path.display()))?;
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(match root {
            LibraryRoot::Library => "Library",
            LibraryRoot::Projects => "Projects",
        })
        .to_string();
    let relative_path = path_to_relative_string(base, path);
    if !metadata.is_dir() {
        return Ok(LibraryEntry {
            name,
            path: relative_path,
            root,
            kind: file_kind(path),
            children: Vec::new(),
        });
    }

    let mut children = Vec::new();
    if remaining_depth > 0 {
        let mut entries = fs::read_dir(path)
            .with_context(|| format!("Failed to read folder {}", path.display()))?
            .collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let child_path = entry.path();
            let child_metadata = fs::symlink_metadata(&child_path)
                .with_context(|| format!("Failed to inspect {}", child_path.display()))?;
            if child_metadata.file_type().is_symlink() {
                continue;
            }
            children.push(read_entry(root, base, &child_path, remaining_depth - 1)?);
        }
    }

    Ok(LibraryEntry {
        name,
        path: relative_path,
        root,
        kind: LibraryEntryKind::Folder,
        children,
    })
}

fn root_path(config: &Config, root: LibraryRoot) -> PathBuf {
    match root {
        LibraryRoot::Library => config.vault_path.clone(),
        LibraryRoot::Projects => config.home.join("Projects"),
    }
}

fn resolve_existing_path(
    config: &Config,
    root: LibraryRoot,
    relative_path: &str,
) -> Result<PathBuf> {
    let path = resolve_new_or_existing_path(config, root, relative_path)?;
    if !path.exists() {
        bail!("Path `{relative_path}` does not exist in {root:?}");
    }
    let canonical_root = canonical_root(config, root)?;
    let canonical_path = path
        .canonicalize()
        .with_context(|| format!("Failed to resolve {}", path.display()))?;
    ensure_inside(&canonical_root, &canonical_path)?;
    Ok(path)
}

fn resolve_new_path(config: &Config, root: LibraryRoot, relative_path: &str) -> Result<PathBuf> {
    let path = resolve_new_or_existing_path(config, root, relative_path)?;
    if path.exists() {
        bail!("Path `{relative_path}` already exists in {root:?}");
    }
    Ok(path)
}

fn resolve_new_or_existing_path(
    config: &Config,
    root: LibraryRoot,
    relative_path: &str,
) -> Result<PathBuf> {
    let relative = normalize_relative_path(relative_path)?;
    let root_path = root_path(config, root);
    fs::create_dir_all(&root_path)
        .with_context(|| format!("Failed to create {}", root_path.display()))?;
    let path = root_path.join(&relative);
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Path `{relative_path}` has no parent"))?;
    let canonical_root = canonical_root(config, root)?;
    let canonical_parent = existing_parent(parent)?;
    ensure_inside(&canonical_root, &canonical_parent)?;
    Ok(path)
}

fn normalize_relative_path(value: &str) -> Result<PathBuf> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("Path must not be empty");
    }
    if trimmed.starts_with('/') || trimmed.starts_with('\\') {
        bail!("Absolute paths are not allowed for library tools");
    }
    let path = Path::new(trimmed);
    if path.is_absolute() {
        bail!("Absolute paths are not allowed for library tools");
    }

    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => out.push(value),
            Component::CurDir => {}
            _ => bail!("Path traversal is not allowed for library tools"),
        }
    }
    if out.as_os_str().is_empty() {
        bail!("Path must contain a file or folder name");
    }
    Ok(out)
}

fn canonical_root(config: &Config, root: LibraryRoot) -> Result<PathBuf> {
    let root_path = root_path(config, root);
    fs::create_dir_all(&root_path)
        .with_context(|| format!("Failed to create {}", root_path.display()))?;
    root_path
        .canonicalize()
        .with_context(|| format!("Failed to resolve {}", root_path.display()))
}

fn existing_parent(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return path
            .canonicalize()
            .with_context(|| format!("Failed to resolve {}", path.display()));
    }
    match path.parent() {
        Some(parent) if parent != path => existing_parent(parent),
        _ => bail!("No existing parent found for {}", path.display()),
    }
}

fn ensure_inside(root: &Path, path: &Path) -> Result<()> {
    if !path.starts_with(root) {
        bail!(
            "Library tool path escaped its sandbox: {} is outside {}",
            path.display(),
            root.display()
        );
    }
    Ok(())
}

fn ensure_markdown(path: &Path) -> Result<()> {
    match path.extension().and_then(|value| value.to_str()) {
        Some(extension) if extension.eq_ignore_ascii_case("md") => Ok(()),
        _ => bail!("Only Markdown .md files in Library can be read or written by this tool"),
    }
}

fn tool_path(config: &Config, root: LibraryRoot, path: &Path) -> ToolPath {
    ToolPath {
        root,
        path: path_to_relative_string(&root_path(config, root), path),
    }
}

fn path_to_relative_string(base: &Path, path: &Path) -> String {
    path.strip_prefix(base)
        .ok()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .filter(|path| !path.is_empty())
        .unwrap_or_else(|| ".".to_string())
}

fn file_kind(path: &Path) -> LibraryEntryKind {
    match path.extension().and_then(|value| value.to_str()) {
        Some(extension) if extension.eq_ignore_ascii_case("md") => LibraryEntryKind::Markdown,
        _ => LibraryEntryKind::File,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn test_config() -> Config {
        let home = std::env::current_dir()
            .expect("current dir")
            .join(format!(".librarian-test-library-tools-{}", Uuid::new_v4()));
        let config = Config::load_or_default(Some(home)).expect("config");
        config.ensure_layout().expect("layout");
        config
    }

    #[test]
    fn blocks_absolute_and_parent_paths() {
        assert!(normalize_relative_path("../outside").is_err());
        assert!(normalize_relative_path("/tmp/outside").is_err());
        assert!(normalize_relative_path("Projects/../../someOtherDirOutside").is_err());
        assert!(normalize_relative_path("notes/ok.md").is_ok());
    }

    #[test]
    fn writes_markdown_only_inside_library() {
        let config = test_config();
        write_markdown(&config, "shelf/book.md", "# Book").expect("write markdown");
        assert_eq!(
            read_markdown(&config, "shelf/book.md").expect("read markdown"),
            "# Book"
        );
        assert!(write_markdown(&config, "shelf/book.txt", "nope").is_err());
        std::fs::remove_dir_all(&config.home).ok();
    }

    #[test]
    fn creates_empty_files_under_projects_without_markdown_write_access() {
        let config = test_config();
        create_folder(&config, LibraryRoot::Projects, "demo").expect("create folder");
        create_empty_file(&config, LibraryRoot::Projects, "demo/.keep").expect("create file");
        assert!(read_markdown(&config, "../Projects/demo/.keep").is_err());
        std::fs::remove_dir_all(&config.home).ok();
    }
}
