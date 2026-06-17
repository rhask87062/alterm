//! Pure layout math for the balanced auto-grid window placement.
//!
//! Wide-first: columns are added before rows so terminals keep their width.
//! Windows fill row-major (left->right across a row, then top->bottom).

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
}
