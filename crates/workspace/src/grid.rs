//! Pure layout math for the balanced auto-grid window placement.
//!
//! Wide-first: columns are added before rows so terminals keep their width.
//! Windows fill row-major (left->right across a row, then top->bottom).

use iced::widget::pane_grid::{self, Configuration};

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
}
