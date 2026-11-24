//! The backend-agnostic model of the changes list.
//!
//! Both the Git panel and the Fossil panel show changed files grouped into
//! sections; only the entry payload differs between backends, so it lives here
//! and both panels render it. The directory tree is Git-only for now, and is
//! generic over the payload so it can follow the entries wherever they go; what
//! depends on Git's index stays in `git_panel`.

use collections::{BTreeMap, HashMap, HashSet};
use git::repository::RepoPath;
use git::status::{DiffStat, FileStatus, StageStatus};
use gpui::{App, SharedString};
use i18n::tr;
use util::paths::PathStyle;

pub(crate) fn section_title(section: Section, cx: &App) -> String {
    match section {
        Section::Conflict => tr(cx, "git_ui.git_panel.section.conflicts", "Conflicts"),
        Section::Tracked => tr(cx, "git_ui.git_panel.section.tracked", "Tracked"),
        Section::New => tr(cx, "git_ui.git_panel.section.untracked", "Untracked"),
    }
}

/// One changed file, with the staging state and diff size the panels show.
///
/// Both panels render this even though only Git has an index: a backend without
/// one leaves every entry unstaged and never renders a staging affordance.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct StatusEntry {
    pub(crate) repo_path: RepoPath,
    pub(crate) status: FileStatus,
    pub(crate) staging: StageStatus,
    pub(crate) diff_stat: Option<DiffStat>,
}

impl StatusEntry {
    pub(crate) fn display_name(&self, path_style: PathStyle) -> String {
        self.repo_path.file_name().map_or_else(
            || self.repo_path.display(path_style).to_string(),
            |name| name.to_owned(),
        )
    }

    pub(crate) fn parent_dir(&self, path_style: PathStyle) -> Option<String> {
        self.repo_path
            .parent()
            .map(|parent| parent.display(path_style).to_string())
    }
}

impl ChangesListFile for StatusEntry {
    fn repo_path(&self) -> &RepoPath {
        &self.repo_path
    }
}

/// The payload of a list row: one changed file.
///
/// The tree builder only needs to know where the file lives; everything else
/// about the payload belongs to the panel that owns it.
pub(crate) trait ChangesListFile {
    fn repo_path(&self) -> &RepoPath;
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub(crate) enum Section {
    Conflict,
    Tracked,
    New,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub(crate) struct SectionHeader {
    pub header: Section,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub(crate) enum ListEntry<T> {
    Status(T),
    TreeStatus(TreeStatusRow<T>),
    Directory(DirectoryRow),
    Header(SectionHeader),
}

impl<T: ChangesListFile> ListEntry<T> {
    pub(crate) fn is_selectable(&self) -> bool {
        matches!(self, ListEntry::Status(_) | ListEntry::TreeStatus(_))
    }

    pub(crate) fn file(&self) -> Option<&T> {
        match self {
            ListEntry::Status(entry) => Some(entry),
            ListEntry::TreeStatus(entry) => Some(&entry.entry),
            _ => None,
        }
    }

    pub(crate) fn directory(&self) -> Option<&DirectoryRow> {
        match self {
            ListEntry::Directory(entry) => Some(entry),
            _ => None,
        }
    }

    /// Returns the tree indentation depth for this entry.
    pub(crate) fn depth(&self) -> usize {
        match self {
            ListEntry::Directory(dir) => dir.depth,
            ListEntry::TreeStatus(status) => status.depth,
            _ => 0,
        }
    }

    pub(crate) fn repo_path(&self) -> Option<&RepoPath> {
        match self {
            ListEntry::Status(entry) => Some(entry.repo_path()),
            ListEntry::TreeStatus(entry) => Some(entry.entry.repo_path()),
            ListEntry::Directory(entry) => Some(&entry.key.path),
            ListEntry::Header(_) => None,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub(crate) struct TreeStatusRow<T> {
    pub entry: T,
    pub depth: usize,
}

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub(crate) struct TreeKey {
    pub section: Section,
    pub path: RepoPath,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub(crate) struct DirectoryRow {
    pub key: TreeKey,
    pub name: SharedString,
    pub depth: usize,
    pub expanded: bool,
}

pub(crate) struct TreeNode<T> {
    name: SharedString,
    path: Option<RepoPath>,
    children: BTreeMap<SharedString, TreeNode<T>>,
    files: Vec<T>,
}

impl<T> Default for TreeNode<T> {
    fn default() -> Self {
        Self {
            name: SharedString::default(),
            path: None,
            children: BTreeMap::new(),
            files: Vec::new(),
        }
    }
}

pub(crate) struct TreeViewState<T> {
    // Maps visible index to actual entry index.
    // Length equals the number of visible entries.
    // This is needed because some entries (like collapsed directories) may be hidden.
    pub(crate) logical_indices: Vec<usize>,
    pub(crate) expanded_dirs: HashMap<TreeKey, bool>,
    pub(crate) directory_descendants: HashMap<TreeKey, Vec<T>>,
}

impl<T> Default for TreeViewState<T> {
    fn default() -> Self {
        Self {
            logical_indices: Vec::new(),
            expanded_dirs: HashMap::default(),
            directory_descendants: HashMap::default(),
        }
    }
}

impl<T: ChangesListFile + Clone> TreeViewState<T> {
    pub(crate) fn build_tree_entries(
        &mut self,
        section: Section,
        mut entries: Vec<T>,
        seen_directories: &mut HashSet<TreeKey>,
    ) -> Vec<(ListEntry<T>, bool)> {
        if entries.is_empty() {
            return Vec::new();
        }

        entries.sort_by(|a, b| a.repo_path().cmp(b.repo_path()));

        let mut root = TreeNode::default();
        for entry in entries {
            let components: Vec<&str> = entry.repo_path().components().collect();
            if components.is_empty() {
                root.files.push(entry);
                continue;
            }

            let mut current = &mut root;
            let mut current_path = String::new();

            for (ix, component) in components.iter().enumerate() {
                if ix == components.len() - 1 {
                    current.files.push(entry.clone());
                } else {
                    if !current_path.is_empty() {
                        current_path.push('/');
                    }
                    current_path.push_str(component);
                    let dir_path = match RepoPath::new(&current_path) {
                        Ok(dir_path) => dir_path,
                        Err(error) => {
                            log::error!(
                                "Failed to build changes tree directory path {current_path:?}: {error}"
                            );
                            continue;
                        }
                    };

                    let component = SharedString::from(component.to_string());

                    current = current
                        .children
                        .entry(component.clone())
                        .or_insert_with(|| TreeNode {
                            name: component,
                            path: Some(dir_path),
                            ..Default::default()
                        });
                }
            }
        }

        let (flattened, _) = self.flatten_tree(&root, section, 0, seen_directories);
        flattened
    }

    fn flatten_tree(
        &mut self,
        node: &TreeNode<T>,
        section: Section,
        depth: usize,
        seen_directories: &mut HashSet<TreeKey>,
    ) -> (Vec<(ListEntry<T>, bool)>, Vec<T>) {
        let mut all_files = Vec::new();
        let mut flattened = Vec::new();

        for child in node.children.values() {
            let (terminal, name) = Self::compact_directory_chain(child);
            let Some(path) = terminal.path.clone().or_else(|| child.path.clone()) else {
                continue;
            };
            let (child_flattened, mut child_files) =
                self.flatten_tree(terminal, section, depth + 1, seen_directories);
            let key = TreeKey { section, path };
            let expanded = *self.expanded_dirs.get(&key).unwrap_or(&true);
            self.expanded_dirs.entry(key.clone()).or_insert(true);
            seen_directories.insert(key.clone());

            self.directory_descendants
                .insert(key.clone(), child_files.clone());

            flattened.push((
                ListEntry::Directory(DirectoryRow {
                    key,
                    name,
                    depth,
                    expanded,
                }),
                true,
            ));

            if expanded {
                flattened.extend(child_flattened);
            } else {
                flattened.extend(child_flattened.into_iter().map(|(child, _)| (child, false)));
            }

            all_files.append(&mut child_files);
        }

        for file in &node.files {
            all_files.push(file.clone());
            flattened.push((
                ListEntry::TreeStatus(TreeStatusRow {
                    entry: file.clone(),
                    depth,
                }),
                true,
            ));
        }

        (flattened, all_files)
    }

    fn compact_directory_chain(mut node: &TreeNode<T>) -> (&TreeNode<T>, SharedString) {
        let mut parts = vec![node.name.clone()];
        while node.files.is_empty() && node.children.len() == 1 {
            let Some(child) = node.children.values().next() else {
                continue;
            };
            if child.path.is_none() {
                break;
            }
            parts.push(child.name.clone());
            node = child;
        }
        let name = parts.join("/");
        (node, SharedString::from(name))
    }
}
