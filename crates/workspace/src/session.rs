//! Serializable session model + capture/restore for persistence.

use std::path::{Path, PathBuf};

use iced::widget::pane_grid::{self, Configuration};
use serde::{Deserialize, Serialize};

use crate::ai_chat::DisplayMessage;
use crate::Block;

pub const SESSION_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SerAxis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BlockState {
    Terminal { cwd: Option<PathBuf>, scrollback_ansi: String, rows: u16, cols: u16 },
    Browser { url: String, history: Vec<String>, history_index: usize },
    AiChat { provider: String, model: String, messages: Vec<DisplayMessage>, input: String },
    Preview { path: PathBuf },
    Note { content: String },
    Settings,
    HotkeyInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PaneNode {
    Split { axis: SerAxis, ratio: f32, a: Box<PaneNode>, b: Box<PaneNode> },
    Leaf(BlockState),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WindowState { pub width: f32, pub height: f32 }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TabState {
    pub title: String,
    pub focus: Option<usize>,
    pub maximized: Option<usize>,
    pub layout: PaneNode,
    /// Custom pane labels as (spatial-order index, label) pairs. Indices match
    /// `panes_in_spatial_order`, the same ordering used for `focus`/`maximized`.
    #[serde(default)]
    pub pane_labels: Vec<(usize, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionState {
    pub version: u32,
    pub window: WindowState,
    pub active_tab: usize,
    pub tabs: Vec<TabState>,
}

impl From<&SerAxis> for pane_grid::Axis {
    fn from(a: &SerAxis) -> Self {
        match a {
            SerAxis::Horizontal => pane_grid::Axis::Horizontal,
            SerAxis::Vertical => pane_grid::Axis::Vertical,
        }
    }
}

fn axis_to_ser(axis: pane_grid::Axis) -> SerAxis {
    match axis {
        pane_grid::Axis::Horizontal => SerAxis::Horizontal,
        pane_grid::Axis::Vertical => SerAxis::Vertical,
    }
}

/// Build an iced pane_grid Configuration from a saved PaneNode tree.
pub fn build_configuration(
    node: &PaneNode,
    make_leaf: &mut dyn FnMut(&BlockState) -> Block,
) -> Configuration<Block> {
    match node {
        PaneNode::Leaf(bs) => Configuration::Pane(make_leaf(bs)),
        PaneNode::Split { axis, ratio, a, b } => Configuration::Split {
            axis: axis.into(),
            ratio: *ratio,
            a: Box::new(build_configuration(a, make_leaf)),
            b: Box::new(build_configuration(b, make_leaf)),
        },
    }
}

/// Capture a PaneNode tree from a live pane_grid State.
pub fn capture_pane_node(
    state: &pane_grid::State<Block>,
    capture_leaf: &mut dyn FnMut(&Block) -> BlockState,
) -> PaneNode {
    capture_node(state.layout(), state, capture_leaf)
}

fn capture_node(
    node: &pane_grid::Node,
    state: &pane_grid::State<Block>,
    capture_leaf: &mut dyn FnMut(&Block) -> BlockState,
) -> PaneNode {
    match node {
        pane_grid::Node::Pane(pane) => {
            let block = state.get(*pane).expect("layout pane exists");
            PaneNode::Leaf(capture_leaf(block))
        }
        pane_grid::Node::Split { axis, ratio, a, b, .. } => PaneNode::Split {
            axis: axis_to_ser(*axis),
            ratio: *ratio,
            a: Box::new(capture_node(a, state, capture_leaf)),
            b: Box::new(capture_node(b, state, capture_leaf)),
        },
    }
}

/// Path to the session file: `<config_dir>/session.json`.
pub fn session_path() -> PathBuf {
    alterm_config::AppConfig::config_dir().join("session.json")
}

/// Write the session atomically (temp file + rename).
pub fn save_to_path(state: &SessionState, path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(state)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json.as_bytes())?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Load the session. Returns `None` (and backs up the file to `*.bak`) on a
/// missing file, parse error, or version mismatch.
pub fn load_from_path(path: &Path) -> Option<SessionState> {
    let bytes = std::fs::read(path).ok()?;
    match serde_json::from_slice::<SessionState>(&bytes) {
        Ok(state) if state.version == SESSION_VERSION => Some(state),
        _ => {
            let bak = path.with_extension("json.bak");
            let _ = std::fs::rename(path, &bak);
            None
        }
    }
}

use crate::grid::panes_in_spatial_order;
use crate::Tab;

/// Snapshot all tabs into a SessionState.
pub fn capture(tabs: &[Tab], active_tab: usize, window: WindowState) -> SessionState {
    let tab_states = tabs.iter().map(capture_tab).collect();
    SessionState { version: SESSION_VERSION, window, active_tab, tabs: tab_states }
}

fn capture_tab(tab: &Tab) -> TabState {
    let order = panes_in_spatial_order(&tab.panes);
    let index_of = |p: pane_grid::Pane| order.iter().position(|q| *q == p);
    let focus = tab.focus.and_then(index_of);
    let maximized = tab.panes.maximized().and_then(index_of);
    let mut capture_leaf = |block: &Block| block.to_block_state();
    let layout = capture_pane_node(&tab.panes, &mut capture_leaf);
    let pane_labels = order
        .iter()
        .enumerate()
        .filter_map(|(i, p)| tab.pane_labels.get(p).map(|l| (i, l.clone())))
        .collect();
    TabState { title: tab.title.clone(), focus, maximized, layout, pane_labels }
}

pub struct RestoredSession {
    pub tabs: Vec<Tab>,
    pub active_tab: usize,
    pub window: WindowState,
}

/// Rebuild live tabs from a SessionState. `config` is needed to reconstruct
/// Settings panes.
pub fn restore(state: SessionState, config: &alterm_config::AppConfig) -> RestoredSession {
    let tabs = state.tabs.into_iter().map(|ts| restore_tab(ts, config)).collect();
    RestoredSession { tabs, active_tab: state.active_tab, window: state.window }
}

fn restore_tab(ts: TabState, config: &alterm_config::AppConfig) -> Tab {
    let mut make_leaf = |bs: &BlockState| Block::from_state(bs, config);
    let cfg = build_configuration(&ts.layout, &mut make_leaf);
    let panes = pane_grid::State::with_configuration(cfg);

    let order = panes_in_spatial_order(&panes);
    let focus = ts.focus.and_then(|i| order.get(i).copied());

    let mut tab = Tab::from_parts(ts.title, panes, focus);
    for (i, label) in ts.pane_labels {
        if let Some(p) = order.get(i).copied() {
            tab.pane_labels.insert(p, label);
        }
    }
    if let Some(i) = ts.maximized {
        if let Some(p) = order.get(i).copied() {
            tab.panes.maximize(p);
        }
    }
    tab
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Tab;

    fn sample() -> SessionState {
        SessionState {
            version: SESSION_VERSION,
            window: WindowState { width: 900.0, height: 600.0 },
            active_tab: 1,
            tabs: vec![
                TabState {
                    title: "one".into(), focus: Some(0), maximized: None,
                    layout: PaneNode::Leaf(BlockState::Preview { path: "/tmp".into() }),
                    pane_labels: vec![],
                },
                TabState {
                    title: "two".into(), focus: Some(1), maximized: Some(0),
                    layout: PaneNode::Split {
                        axis: SerAxis::Vertical, ratio: 0.5,
                        a: Box::new(PaneNode::Leaf(BlockState::Browser {
                            url: "https://example.com".into(),
                            history: vec!["https://example.com".into()], history_index: 0,
                        })),
                        b: Box::new(PaneNode::Leaf(BlockState::AiChat {
                            provider: "openai".into(), model: "gpt-4o".into(),
                            messages: vec![], input: "hi".into(),
                        })),
                    },
                    pane_labels: vec![(0, "left".into())],
                },
            ],
        }
    }

    #[test]
    fn session_state_json_round_trip() {
        let s = sample();
        let json = serde_json::to_string(&s).unwrap();
        let back: SessionState = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn pane_node_round_trips_through_configuration_preserving_structure() {
        // Leaf -> bare Pane; Split preserves axis + ratio + structure.
        let node = PaneNode::Split {
            axis: SerAxis::Horizontal, ratio: 0.25,
            a: Box::new(PaneNode::Leaf(BlockState::Settings)),
            b: Box::new(PaneNode::Leaf(BlockState::HotkeyInfo)),
        };
        let mut make = |_bs: &BlockState| Block::new_hotkey_info();
        let cfg = build_configuration(&node, &mut make);
        match cfg {
            Configuration::Split { axis, ratio, .. } => {
                assert_eq!(axis, pane_grid::Axis::Horizontal);
                assert!((ratio - 0.25).abs() < 1e-6);
            }
            _ => panic!("expected split"),
        }
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = std::env::temp_dir().join(format!("alterm-sess-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.json");
        let s = sample();
        save_to_path(&s, &path).unwrap();
        let back = load_from_path(&path).expect("loadable");
        assert_eq!(s, back);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_file_returns_none_and_backs_up() {
        let dir = std::env::temp_dir().join(format!("alterm-sess-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.json");
        std::fs::write(&path, b"{ not valid json").unwrap();
        assert!(load_from_path(&path).is_none());
        assert!(dir.join("session.json.bak").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn version_mismatch_returns_none() {
        let dir = std::env::temp_dir().join(format!("alterm-sess-ver-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.json");
        let mut s = sample();
        s.version = 999;
        save_to_path(&s, &path).unwrap();
        assert!(load_from_path(&path).is_none());
        assert!(dir.join("session.json.bak").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn capture_reads_browser_and_preview_blocks() {
        // Build a tab whose single pane is a browser, then capture.
        let mut tab = Tab::new().unwrap();
        // Replace the single pane's block with a browser.
        let pane = *tab.panes.iter().next().unwrap().0;
        *tab.panes.get_mut(pane).unwrap() = Block::new_browser("https://example.com");
        let session = capture(&[tab], 0, WindowState { width: 800.0, height: 600.0 });
        assert_eq!(session.version, SESSION_VERSION);
        assert_eq!(session.tabs.len(), 1);
        match &session.tabs[0].layout {
            PaneNode::Leaf(BlockState::Browser { url, .. }) => {
                assert!(url.contains("example.com"));
            }
            other => panic!("expected browser leaf, got {other:?}"),
        }
    }

    #[test]
    fn restore_rebuilds_tabs_and_focus() {
        let s = SessionState {
            version: SESSION_VERSION,
            window: WindowState { width: 1000.0, height: 700.0 },
            active_tab: 0,
            tabs: vec![TabState {
                title: "restored".into(),
                focus: Some(1),
                maximized: None,
                layout: PaneNode::Split {
                    axis: SerAxis::Vertical, ratio: 0.5,
                    a: Box::new(PaneNode::Leaf(BlockState::Preview { path: "/tmp".into() })),
                    b: Box::new(PaneNode::Leaf(BlockState::HotkeyInfo)),
                },
                pane_labels: vec![],
            }],
        };
        let restored = restore(s, &alterm_config::AppConfig::default());
        assert_eq!(restored.active_tab, 0);
        assert_eq!(restored.window.width, 1000.0);
        assert_eq!(restored.tabs.len(), 1);
        let tab = &restored.tabs[0];
        assert_eq!(tab.title, "restored");
        assert_eq!(tab.panes.len(), 2);
        // focus index 1 maps to the second pane in spatial order.
        let order = crate::grid::panes_in_spatial_order(&tab.panes);
        assert_eq!(tab.focus, Some(order[1]));
    }
}
