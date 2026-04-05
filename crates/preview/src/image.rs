/// Attempt to read image dimensions from the file header bytes.
///
/// Returns `Some((width, height))` if the format is recognised, or `None`
/// if the format is unknown or the file is too short.  Full image decoding
/// is deferred to a future enhancement.
pub fn image_info(_path: &std::path::Path) -> Option<(u32, u32)> {
    // Image display is a future enhancement — return None for now.
    None
}
