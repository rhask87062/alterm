//! Pure layout math for the balanced auto-grid window placement.
//!
//! Wide-first: columns are added before rows so terminals keep their width.
//! Windows fill row-major (left->right across a row, then top->bottom).

use iced::widget::pane_grid::{self, Configuration, Pane, State};
use iced::{Rectangle, Size};

/// Compute `(rows, cols)` for a wide-first balanced grid of `n` windows.
///
/// `cols = ceil(sqrt(n))`, `rows = ceil(n / cols)`. Returns `(0, 0)` for `n == 0`.
pub fn grid_dims(n: usize) -> (usize, usize) {
    if n == 0 {
        return (0, 0);
    }
    let cols = (n as f64).sqrt().ceil() as usize;
    let rows = (n + cols - 1) / cols; // ceil(n / cols)
    (rows, cols)
}

/// Build a wide-first, row-major balanced grid `Configuration` from `items`.
///
/// Items fill left->right across a row, then top->bottom across rows, with even
/// split ratios. Panics if `items` is empty.
pub fn build_grid_config<T>(items: Vec<T>) -> Configuration<T> {
    assert!(!items.is_empty(), "build_grid_config requires at least one item");
    let (_rows, cols) = grid_dims(items.len());

    // Chunk items into rows of up to `cols` columns; each row is a Vertical chain.
    let mut row_configs: Vec<Configuration<T>> = Vec::new();
    let mut buf: Vec<Configuration<T>> = Vec::with_capacity(cols);
    for item in items {
        buf.push(Configuration::Pane(item));
        if buf.len() == cols {
            row_configs.push(combine(std::mem::take(&mut buf), pane_grid::Axis::Vertical));
        }
    }
    if !buf.is_empty() {
        row_configs.push(combine(buf, pane_grid::Axis::Vertical));
    }

    // Stack the rows top->bottom with a Horizontal chain.
    combine(row_configs, pane_grid::Axis::Horizontal)
}

/// Panes sorted into row-major spatial order (top->bottom, then left->right).
pub fn panes_in_spatial_order<T>(state: &State<T>) -> Vec<Pane> {
    // spacing/min_size = 0 so tiny grids aren't distorted by clamping; the bounds
    // value is arbitrary because ordering is scale-invariant.
    let regions = state
        .layout()
        .pane_regions(0.0, 0.0, Size::new(1000.0, 1000.0));
    let mut entries: Vec<(Pane, Rectangle)> = regions.into_iter().collect();
    entries.sort_by(|(_, a), (_, b)| {
        a.y.partial_cmp(&b.y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
    });
    entries.into_iter().map(|(p, _)| p).collect()
}

/// Result of rebuilding a tab's layout into a balanced grid.
pub struct RebuildInfo {
    /// `(old_pane, new_pane)` for each pre-existing window, in spatial order.
    pub remap: Vec<(Pane, Pane)>,
    /// The pane holding the newly added window.
    pub new_pane: Pane,
}

/// Drain every window from `state` in spatial order, append `new_item`, and
/// replace `state` with a freshly built wide-first balanced grid.
///
/// `placeholder` produces throwaway values used to move owned items out of the
/// old state (for `Block`, pass `|| Block::HotkeyInfo`).
pub fn rebuild_with_new<T>(
    state: &mut State<T>,
    new_item: T,
    mut placeholder: impl FnMut() -> T,
) -> RebuildInfo {
    let old_order = panes_in_spatial_order(state);

    let mut items: Vec<T> = Vec::with_capacity(old_order.len() + 1);
    for &pane in &old_order {
        let slot = state.get_mut(pane).expect("pane from layout must exist");
        items.push(std::mem::replace(slot, placeholder()));
    }
    items.push(new_item);

    *state = State::with_configuration(build_grid_config(items));

    let new_order = panes_in_spatial_order(state);
    let remap = old_order
        .iter()
        .copied()
        .zip(new_order.iter().copied())
        .collect();
    let new_pane = *new_order.last().expect("rebuilt grid has at least one pane");

    RebuildInfo { remap, new_pane }
}

/// Combine `configs` along `axis` into one `Configuration` with even ratios,
/// as a right-leaning chain ([a, b, c] -> Split(a, Split(b, c))).
fn combine<T>(configs: Vec<Configuration<T>>, axis: pane_grid::Axis) -> Configuration<T> {
    let mut iter = configs.into_iter().rev();
    let mut acc = iter.next().expect("combine requires at least one config");
    let mut count = 1usize;
    for cfg in iter {
        count += 1;
        let ratio = 1.0 / count as f32; // first element of a `count`-chain gets 1/count
        acc = Configuration::Split {
            axis,
            ratio,
            a: Box::new(cfg),
            b: Box::new(acc),
        };
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_dims_matches_spec() {
        assert_eq!(grid_dims(0), (0, 0));
        assert_eq!(grid_dims(1), (1, 1));
        assert_eq!(grid_dims(2), (1, 2)); // side by side
        assert_eq!(grid_dims(3), (2, 2));
        assert_eq!(grid_dims(4), (2, 2));
        assert_eq!(grid_dims(5), (2, 3));
        assert_eq!(grid_dims(6), (2, 3));
        assert_eq!(grid_dims(7), (3, 3));
        assert_eq!(grid_dims(9), (3, 3));
        assert_eq!(grid_dims(10), (3, 4));
    }

    /// Collect leaf values of a Configuration in DFS order (a before b).
    fn leaves<T: Clone>(cfg: &Configuration<T>) -> Vec<T> {
        match cfg {
            Configuration::Pane(v) => vec![v.clone()],
            Configuration::Split { a, b, .. } => {
                let mut out = leaves(a);
                out.extend(leaves(b));
                out
            }
        }
    }

    #[test]
    fn single_item_is_a_bare_pane() {
        let cfg = build_grid_config(vec![1u32]);
        assert!(matches!(cfg, Configuration::Pane(1)));
    }

    #[test]
    fn two_items_split_into_one_row() {
        // N=2 -> 1 row, 2 cols -> a single Vertical split, items left->right.
        let cfg = build_grid_config(vec![1u32, 2]);
        match cfg {
            Configuration::Split { axis, ref a, ref b, .. } => {
                assert_eq!(axis, pane_grid::Axis::Vertical);
                assert!(matches!(**a, Configuration::Pane(1)));
                assert!(matches!(**b, Configuration::Pane(2)));
            }
            _ => panic!("expected a split"),
        }
    }

    #[test]
    fn outer_split_is_horizontal_for_multi_row() {
        // N=3 -> 2 rows: outer split must be Horizontal (rows stacked).
        let cfg = build_grid_config(vec![1u32, 2, 3]);
        assert!(matches!(cfg, Configuration::Split { axis, .. } if axis == pane_grid::Axis::Horizontal));
    }

    #[test]
    fn leaf_order_is_row_major_for_n_up_to_9() {
        for n in 1..=9usize {
            let items: Vec<u32> = (0..n as u32).collect();
            let cfg = build_grid_config(items.clone());
            assert_eq!(leaves(&cfg), items, "row-major order broken for n={n}");
        }
    }

    /// Spatially-ordered contents of a State<u32>.
    fn ordered_contents(state: &State<u32>) -> Vec<u32> {
        panes_in_spatial_order(state)
            .iter()
            .map(|p| *state.get(*p).unwrap())
            .collect()
    }

    #[test]
    fn rebuild_appends_and_preserves_order() {
        // Start with a single window holding 10.
        let (mut state, _first) = State::new(10u32);
        // Add 20, 30, 40 one at a time.
        for v in [20u32, 30, 40] {
            rebuild_with_new(&mut state, v, || 0u32);
        }
        assert_eq!(state.len(), 4);
        assert_eq!(ordered_contents(&state), vec![10, 20, 30, 40]);
    }

    #[test]
    fn rebuild_reports_new_pane_and_remap() {
        let (mut state, _first) = State::new(1u32);
        let info = rebuild_with_new(&mut state, 2u32, || 0u32);
        // One pre-existing window -> one remap pair.
        assert_eq!(info.remap.len(), 1);
        // new_pane holds the new item.
        assert_eq!(*state.get(info.new_pane).unwrap(), 2);
        // Each remap target still exists in the new state.
        for (_old, new) in &info.remap {
            assert!(state.get(*new).is_some());
        }
        // Verify the remap target holds the pre-existing content.
        assert_eq!(*state.get(info.remap[0].1).unwrap(), 1);
        // Verify the remap target is distinct from the new pane.
        assert_ne!(info.remap[0].1, info.new_pane);
    }

    #[test]
    fn rebuild_remap_targets_hold_preexisting_content() {
        let (mut state, _first) = State::new(1u32);
        // Add window 2.
        rebuild_with_new(&mut state, 2u32, || 0u32);
        // Add window 3 and capture the remap info.
        let info = rebuild_with_new(&mut state, 3u32, || 0u32);
        // Two pre-existing windows: holding 1 and 2.
        assert_eq!(info.remap.len(), 2);
        // Remap targets hold pre-existing content in order [1, 2].
        let remap_contents: Vec<u32> = info
            .remap
            .iter()
            .map(|(_, new)| *state.get(*new).unwrap())
            .collect();
        assert_eq!(remap_contents, vec![1, 2]);
        // new_pane holds the newly added window 3.
        assert_eq!(*state.get(info.new_pane).unwrap(), 3);
        // Final state has all three windows in order.
        assert_eq!(ordered_contents(&state), vec![1, 2, 3]);
    }
}
