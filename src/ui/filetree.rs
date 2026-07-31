//! Collapsible file tree sidebar for browsing decrypted folder archives.

use std::collections::BTreeMap;

use egui::{collapsing_header::CollapsingState, RichText, Ui};

use super::theme;
use crate::document::archive::ArchiveEntry;
use crate::document::FileType;

/// A directory node in the tree. Files carry their index into the
/// archive's entry list; `full_path` is the folder's vault-relative path
/// (empty for the synthetic root) so a click can report which folder.
#[derive(Default)]
pub struct TreeNode {
    full_path: String,
    dirs: BTreeMap<String, TreeNode>,
    files: Vec<(String, usize)>,
}

/// Build a tree from the archive's (sorted) entries plus any empty
/// directory entries, so a folder with no files still appears.
pub fn build_tree(entries: &[ArchiveEntry], dirs: &[String]) -> TreeNode {
    let mut root = TreeNode::default();

    // Ensure a folder path (and its ancestors) exist as nodes.
    fn ensure_dir<'a>(root: &'a mut TreeNode, path: &str) -> &'a mut TreeNode {
        let mut node = root;
        let mut acc = String::new();
        for part in path.split('/').filter(|p| !p.is_empty()) {
            if acc.is_empty() {
                acc = part.to_string();
            } else {
                acc = format!("{acc}/{part}");
            }
            node = node.dirs.entry(part.to_string()).or_default();
            if node.full_path.is_empty() {
                node.full_path = acc.clone();
            }
        }
        node
    }

    for dir in dirs {
        ensure_dir(&mut root, dir);
    }

    for (idx, entry) in entries.iter().enumerate() {
        let parts: Vec<&str> = entry
            .rel_path
            .split('/')
            .filter(|p| !p.is_empty())
            .collect();
        if parts.is_empty() {
            continue;
        }
        let parent = if parts.len() == 1 {
            &mut root
        } else {
            ensure_dir(&mut root, &parts[..parts.len() - 1].join("/"))
        };
        parent.files.push((parts[parts.len() - 1].to_string(), idx));
    }
    root
}

/// A click result from the tree.
pub enum TreeClick {
    /// A file was clicked (its entry index).
    File(usize),
    /// A folder header's name was clicked (its vault-relative path).
    Folder(String),
}

/// Render the tree. `selected` is the open file's index; `selected_dir`
/// is the highlighted folder (for folder operations). Returns what the
/// user clicked, if anything.
pub fn render(
    ui: &mut Ui,
    tree: &TreeNode,
    entries: &[ArchiveEntry],
    selected: usize,
    selected_dir: Option<&str>,
) -> Option<TreeClick> {
    let mut clicked = None;
    render_node(ui, tree, entries, selected, selected_dir, &mut clicked);
    clicked
}

fn render_node(
    ui: &mut Ui,
    node: &TreeNode,
    entries: &[ArchiveEntry],
    selected: usize,
    selected_dir: Option<&str>,
    clicked: &mut Option<TreeClick>,
) {
    for (name, child) in &node.dirs {
        // CollapsingState paints its own disclosure triangle (a real font
        // triangle would be tofu in the bundled fonts) and lets the header
        // be a selectable label, so a folder can be picked for rename or
        // delete without collapsing it.
        let id = ui.make_persistent_id(("vault_dir", &child.full_path));
        let is_dir_sel = selected_dir == Some(child.full_path.as_str());
        CollapsingState::load_with_default_open(ui.ctx(), id, true)
            .show_header(ui, |ui| {
                let label =
                    RichText::new(format!("\u{1F5C0} {name}"))
                        .size(13.0)
                        .color(if is_dir_sel {
                            theme::accent()
                        } else {
                            theme::text_primary()
                        });
                if ui.selectable_label(is_dir_sel, label).clicked() {
                    *clicked = Some(TreeClick::Folder(child.full_path.clone()));
                }
            })
            .body(|ui| {
                render_node(ui, child, entries, selected, selected_dir, clicked);
            });
    }

    for (name, idx) in &node.files {
        let is_selected = *idx == selected;
        let icon = match entries[*idx].file_type {
            FileType::Markdown => "\u{1F4DD}",
            FileType::PlainText => "\u{1F4C4}",
        };
        let text = RichText::new(format!("{icon} {name}"))
            .size(13.0)
            .color(if is_selected {
                theme::accent()
            } else {
                theme::text_primary()
            });
        if ui.selectable_label(is_selected, text).clicked() {
            *clicked = Some(TreeClick::File(*idx));
        }
    }
}
