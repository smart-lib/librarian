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

#[derive(Clone, Debug, Serialize)]
pub struct MarkdownSlice {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub total_lines: usize,
    pub content: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct MarkdownMatch {
    pub path: String,
    pub line_number: usize,
    pub line: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct MarkdownEdit {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub removed: String,
    pub total_lines_before: usize,
    pub total_lines_after: usize,
}

pub fn tree(config: &Config, root: LibraryRoot, max_depth: usize) -> Result<LibraryEntry> {
    let root_path = root_path(config, root);
    fs::create_dir_all(&root_path)
        .with_context(|| format!("Failed to create {}", root_path.display()))?;
    read_entry(root, &root_path, &root_path, max_depth.min(12))
}

pub fn normalize_tool_relative_path(relative_path: &str) -> Result<String> {
    Ok(normalize_relative_path(relative_path)?
        .to_string_lossy()
        .replace('\\', "/"))
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

pub fn read_markdown_lines(
    config: &Config,
    relative_path: &str,
    start_line: usize,
    end_line: usize,
) -> Result<MarkdownSlice> {
    let path = resolve_existing_path(config, LibraryRoot::Library, relative_path)?;
    ensure_markdown(&path)?;
    let content =
        fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;
    let lines = split_lines_preserve(&content);
    let (start, end) = normalize_line_range(start_line, end_line, lines.len())?;
    Ok(MarkdownSlice {
        path: path_to_relative_string(&root_path(config, LibraryRoot::Library), &path),
        start_line: start,
        end_line: end,
        total_lines: lines.len(),
        content: lines[start - 1..end].concat(),
    })
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

pub fn append_markdown(config: &Config, relative_path: &str, content: &str) -> Result<ToolPath> {
    let path = resolve_new_or_existing_path(config, LibraryRoot::Library, relative_path)?;
    ensure_markdown(&path)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create folder {}", parent.display()))?;
    }
    let mut current = if path.exists() {
        fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?
    } else {
        String::new()
    };
    if !current.is_empty() && !current.ends_with('\n') {
        current.push('\n');
    }
    current.push_str(content);
    fs::write(&path, current).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(tool_path(config, LibraryRoot::Library, &path))
}

pub fn replace_markdown_lines(
    config: &Config,
    relative_path: &str,
    start_line: usize,
    end_line: usize,
    replacement: &str,
) -> Result<MarkdownEdit> {
    edit_markdown_lines(
        config,
        relative_path,
        start_line,
        end_line,
        Some(replacement),
    )
}

pub fn cut_markdown_lines(
    config: &Config,
    relative_path: &str,
    start_line: usize,
    end_line: usize,
) -> Result<MarkdownEdit> {
    edit_markdown_lines(config, relative_path, start_line, end_line, None)
}

pub fn find_markdown(
    config: &Config,
    relative_path: &str,
    query: &str,
    limit: usize,
) -> Result<Vec<MarkdownMatch>> {
    if query.is_empty() {
        bail!("Search query must not be empty");
    }
    let path = resolve_existing_path(config, LibraryRoot::Library, relative_path)?;
    ensure_markdown(&path)?;
    let content =
        fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;
    let relative = path_to_relative_string(&root_path(config, LibraryRoot::Library), &path);
    let query_lower = query.to_ascii_lowercase();
    let mut matches = Vec::new();
    for (index, line) in content.lines().enumerate() {
        if line.to_ascii_lowercase().contains(&query_lower) {
            matches.push(MarkdownMatch {
                path: relative.clone(),
                line_number: index + 1,
                line: line.to_string(),
            });
            if matches.len() >= limit.max(1) {
                break;
            }
        }
    }
    Ok(matches)
}

pub fn replace_first_markdown_match(
    config: &Config,
    relative_path: &str,
    query: &str,
    replacement: &str,
) -> Result<MarkdownEdit> {
    let line = first_match_line(config, relative_path, query)?;
    replace_markdown_lines(config, relative_path, line, line, replacement)
}

pub fn cut_first_markdown_match(
    config: &Config,
    relative_path: &str,
    query: &str,
) -> Result<MarkdownEdit> {
    let line = first_match_line(config, relative_path, query)?;
    cut_markdown_lines(config, relative_path, line, line)
}

pub fn replace_markdown_section(
    config: &Config,
    relative_path: &str,
    heading: &str,
    replacement: &str,
) -> Result<MarkdownEdit> {
    let (start, end) = markdown_section_range(config, relative_path, heading)?;
    replace_markdown_lines(config, relative_path, start, end, replacement)
}

pub fn cut_markdown_section(
    config: &Config,
    relative_path: &str,
    heading: &str,
) -> Result<MarkdownEdit> {
    let (start, end) = markdown_section_range(config, relative_path, heading)?;
    cut_markdown_lines(config, relative_path, start, end)
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

fn edit_markdown_lines(
    config: &Config,
    relative_path: &str,
    start_line: usize,
    end_line: usize,
    replacement: Option<&str>,
) -> Result<MarkdownEdit> {
    let path = resolve_existing_path(config, LibraryRoot::Library, relative_path)?;
    ensure_markdown(&path)?;
    let content =
        fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;
    let mut lines = split_lines_preserve(&content);
    let total_lines_before = lines.len();
    let (start, end) = normalize_line_range(start_line, end_line, total_lines_before)?;
    let removed = lines[start - 1..end].concat();
    let replacement_lines = replacement
        .map(|replacement| {
            let mut replacement = replacement.to_string();
            if removed.ends_with('\n') && !replacement.ends_with('\n') {
                replacement.push('\n');
            }
            split_lines_preserve(&replacement)
        })
        .unwrap_or_default();
    lines.splice(start - 1..end, replacement_lines);
    fs::write(&path, lines.concat())
        .with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(MarkdownEdit {
        path: path_to_relative_string(&root_path(config, LibraryRoot::Library), &path),
        start_line: start,
        end_line: end,
        removed,
        total_lines_before,
        total_lines_after: lines.len(),
    })
}

fn first_match_line(config: &Config, relative_path: &str, query: &str) -> Result<usize> {
    find_markdown(config, relative_path, query, 1)?
        .first()
        .map(|item| item.line_number)
        .ok_or_else(|| anyhow::anyhow!("No match found for `{query}`"))
}

fn markdown_section_range(
    config: &Config,
    relative_path: &str,
    heading: &str,
) -> Result<(usize, usize)> {
    let path = resolve_existing_path(config, LibraryRoot::Library, relative_path)?;
    ensure_markdown(&path)?;
    let content =
        fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;
    let lines = split_lines_preserve(&content);
    let heading = normalize_heading_text(heading);
    if heading.is_empty() {
        bail!("Markdown heading must not be empty");
    }
    let mut start = None;
    let mut level = 0_usize;
    for (index, line) in lines.iter().enumerate() {
        if let Some((line_level, title)) = parse_heading_line(line) {
            if start.is_none() && normalize_heading_text(title) == heading {
                start = Some(index + 1);
                level = line_level;
                continue;
            }
            if start.is_some() && line_level <= level {
                return Ok((start.unwrap(), index));
            }
        }
    }
    start
        .map(|start| (start, lines.len().max(start)))
        .ok_or_else(|| anyhow::anyhow!("Markdown section `{heading}` was not found"))
}

fn parse_heading_line(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim_start();
    let level = trimmed.chars().take_while(|ch| *ch == '#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let rest = &trimmed[level..];
    if !rest.starts_with(' ') {
        return None;
    }
    Some((level, rest.trim()))
}

fn normalize_heading_text(value: &str) -> String {
    value
        .trim()
        .trim_start_matches('#')
        .trim()
        .to_ascii_lowercase()
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

fn split_lines_preserve(content: &str) -> Vec<String> {
    if content.is_empty() {
        return Vec::new();
    }
    content
        .split_inclusive('\n')
        .map(ToOwned::to_owned)
        .collect()
}

fn normalize_line_range(
    start_line: usize,
    end_line: usize,
    total_lines: usize,
) -> Result<(usize, usize)> {
    if total_lines == 0 {
        bail!("Markdown file is empty");
    }
    if start_line == 0 || end_line == 0 {
        bail!("Line numbers are 1-based");
    }
    if start_line > end_line {
        bail!("Start line must be before or equal to end line");
    }
    if end_line > total_lines {
        bail!("Line range {start_line}-{end_line} exceeds file length {total_lines}");
    }
    Ok((start_line, end_line))
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
    fn edits_markdown_by_line_range_without_rewriting_callers_buffer() {
        let config = test_config();
        write_markdown(&config, "shelf/book.md", "one\ntwo\nthree\n").expect("write markdown");
        let slice = read_markdown_lines(&config, "shelf/book.md", 2, 2).expect("read lines");
        assert_eq!(slice.content, "two\n");

        let edit =
            replace_markdown_lines(&config, "shelf/book.md", 2, 2, "TWO\n").expect("replace line");
        assert_eq!(edit.removed, "two\n");
        assert_eq!(
            read_markdown(&config, "shelf/book.md").expect("read markdown"),
            "one\nTWO\nthree\n"
        );
        replace_markdown_lines(&config, "shelf/book.md", 2, 2, "two again")
            .expect("replace line without explicit newline");
        assert_eq!(
            read_markdown(&config, "shelf/book.md").expect("read markdown"),
            "one\ntwo again\nthree\n"
        );

        let edit = cut_markdown_lines(&config, "shelf/book.md", 3, 3).expect("cut line");
        assert_eq!(edit.removed, "three\n");
        assert_eq!(
            read_markdown(&config, "shelf/book.md").expect("read markdown"),
            "one\ntwo again\n"
        );
        std::fs::remove_dir_all(&config.home).ok();
    }

    #[test]
    fn edits_markdown_by_search_match() {
        let config = test_config();
        write_markdown(&config, "shelf/book.md", "alpha\nbeta\ngamma\n").expect("write markdown");
        let matches = find_markdown(&config, "shelf/book.md", "BET", 5).expect("find");
        assert_eq!(matches[0].line_number, 2);

        replace_first_markdown_match(&config, "shelf/book.md", "beta", "BETA\n")
            .expect("replace match");
        cut_first_markdown_match(&config, "shelf/book.md", "gamma").expect("cut match");
        assert_eq!(
            read_markdown(&config, "shelf/book.md").expect("read markdown"),
            "alpha\nBETA\n"
        );
        std::fs::remove_dir_all(&config.home).ok();
    }

    #[test]
    fn edits_markdown_by_section_heading() {
        let config = test_config();
        write_markdown(
            &config,
            "shelf/book.md",
            "# Book\n\n## Target\nold\n\n### Child\nchild\n\n## Next\nnext\n",
        )
        .expect("write markdown");
        let edit = replace_markdown_section(&config, "shelf/book.md", "Target", "## Target\nnew\n")
            .expect("replace section");
        assert_eq!(edit.start_line, 3);
        assert_eq!(
            read_markdown(&config, "shelf/book.md").expect("read markdown"),
            "# Book\n\n## Target\nnew\n## Next\nnext\n"
        );
        cut_markdown_section(&config, "shelf/book.md", "Next").expect("cut section");
        assert_eq!(
            read_markdown(&config, "shelf/book.md").expect("read markdown"),
            "# Book\n\n## Target\nnew\n"
        );
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
