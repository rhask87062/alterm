/// PPTX slide rendering via LibreOffice + pdftoppm/ImageMagick.
///
/// LibreOffice's `--convert-to png` only exports the *first* slide, so we
/// use a two-step pipeline:
///   1. LibreOffice → PDF  (all slides as pages)
///   2. pdftoppm or ImageMagick → one PNG per page

use std::path::{Path, PathBuf};
use std::process::Command;

/// Convert a PPTX file to a list of per-slide PNG images.
///
/// Returns `(image_paths, temp_dir)` on success.  The caller owns the temp
/// directory and must delete it via `std::fs::remove_dir_all` when done.
pub fn render_slides(path: &Path) -> Result<(Vec<PathBuf>, PathBuf), String> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    let temp_dir = std::env::temp_dir().join(format!("alterm-pptx-{ts:x}"));
    std::fs::create_dir_all(&temp_dir)
        .map_err(|e| format!("Cannot create temp dir: {e}"))?;

    // Step 1 — PPTX → PDF (all slides become PDF pages).
    let pdf_path = convert_to_pdf(path, &temp_dir)?;

    // Step 2 — PDF → one PNG per page.
    let images = pdf_to_pngs(&pdf_path, &temp_dir)?;

    Ok((images, temp_dir))
}

// ---------------------------------------------------------------------------
// Step 1: PPTX → PDF via LibreOffice
// ---------------------------------------------------------------------------

fn convert_to_pdf(input: &Path, out_dir: &Path) -> Result<PathBuf, String> {
    let out_dir_str = out_dir.to_str().ok_or("temp dir path is not UTF-8")?;
    let input_str = input.to_str().ok_or("input path is not UTF-8")?;

    let output = Command::new("libreoffice")
        .args(["--headless", "--convert-to", "pdf", "--outdir", out_dir_str, input_str])
        .output()
        .map_err(|e| format!(
            "Failed to launch LibreOffice: {e}\n\nMake sure 'libreoffice' is installed."
        ))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("LibreOffice PDF conversion failed:\n{stderr}"));
    }

    // LibreOffice names the output file after the input stem.
    let stem = input
        .file_stem()
        .ok_or("Input has no filename")?
        .to_string_lossy();
    let pdf = out_dir.join(format!("{stem}.pdf"));

    if pdf.exists() {
        return Ok(pdf);
    }

    // Some LibreOffice versions may vary casing — scan for any PDF in the dir.
    std::fs::read_dir(out_dir)
        .map_err(|e| format!("Cannot read temp dir: {e}"))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.extension().map_or(false, |x| x.eq_ignore_ascii_case("pdf")))
        .ok_or_else(|| "LibreOffice ran but produced no PDF file.".into())
}

// ---------------------------------------------------------------------------
// Step 2: PDF → PNGs (one per page)
// ---------------------------------------------------------------------------

fn pdf_to_pngs(pdf: &Path, out_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let pdf_str = pdf.to_str().ok_or("PDF path is not UTF-8")?;

    // -- Try pdftoppm (poppler-utils) first — very common on Linux desktops.
    let slide_prefix = out_dir.join("slide");
    let slide_prefix_str = slide_prefix.to_str().ok_or("prefix path is not UTF-8")?;

    let pdftoppm = Command::new("pdftoppm")
        .args(["-png", "-r", "150", pdf_str, slide_prefix_str])
        .status();

    if pdftoppm.map(|s| s.success()).unwrap_or(false) {
        let images = collect_pngs(out_dir)?;
        if !images.is_empty() {
            return Ok(images);
        }
    }

    // -- Fall back to ImageMagick convert.
    let pattern = out_dir.join("slide-%04d.png");
    let pattern_str = pattern.to_str().ok_or("pattern path is not UTF-8")?;

    let magick = Command::new("convert")
        .args(["-density", "150", pdf_str, pattern_str])
        .status();

    if magick.map(|s| s.success()).unwrap_or(false) {
        let images = collect_pngs(out_dir)?;
        if !images.is_empty() {
            return Ok(images);
        }
    }

    Err(
        "Could not convert slides to images.\n\n\
         Install one of:\n\
         • poppler-utils  (sudo apt install poppler-utils)\n\
         • ImageMagick    (sudo apt install imagemagick)"
            .into(),
    )
}

fn collect_pngs(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut images: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("Cannot read temp dir: {e}"))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map_or(false, |x| x.eq_ignore_ascii_case("png")))
        .collect();

    // Sort by filename — both pdftoppm and ImageMagick produce numeric suffixes.
    images.sort_by(|a, b| {
        a.file_name()
            .unwrap_or_default()
            .cmp(b.file_name().unwrap_or_default())
    });

    Ok(images)
}
