pub mod code;
pub mod image;

pub use code::{HighlightedLine, HighlightedSpan};

/// The type of content held in a [`PreviewState`].
pub enum FileType {
    Code { language: String },
    Markdown,
    Image,
    Text,
    Directory,
    Unsupported,
}

/// Rendered content produced for a given file.
pub enum PreviewContent {
    Text(String),
    HighlightedCode(Vec<HighlightedLine>),
    Directory(Vec<DirEntry>),
    ImageInfo { width: u32, height: u32 },
    /// Holds a human-readable error / reason when the file cannot be shown.
    Unsupported(String),
}

/// A single entry inside a directory listing.
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

/// All state needed to display a file (or directory) in the preview pane.
pub struct PreviewState {
    pub path: std::path::PathBuf,
    pub file_type: FileType,
    pub content: PreviewContent,
    pub scroll_offset: usize,
}

// ---------------------------------------------------------------------------
// File-type detection
// ---------------------------------------------------------------------------

fn detect_file_type(path: &std::path::Path) -> FileType {
    if path.is_dir() {
        return FileType::Directory;
    }

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase());

    match ext.as_deref() {
        Some("rs") => FileType::Code { language: "Rust".into() },
        Some("py") => FileType::Code { language: "Python".into() },
        Some("js") | Some("jsx") => FileType::Code { language: "JavaScript".into() },
        Some("ts") | Some("tsx") => FileType::Code { language: "TypeScript".into() },
        Some("go") => FileType::Code { language: "Go".into() },
        Some("c") | Some("h") => FileType::Code { language: "C".into() },
        Some("cpp") | Some("hpp") | Some("cc") => FileType::Code { language: "C++".into() },
        Some("java") => FileType::Code { language: "Java".into() },
        Some("toml") => FileType::Code { language: "TOML".into() },
        Some("json") => FileType::Code { language: "JSON".into() },
        Some("yaml") | Some("yml") => FileType::Code { language: "YAML".into() },
        Some("html") => FileType::Code { language: "HTML".into() },
        Some("css") => FileType::Code { language: "CSS".into() },
        Some("sh") | Some("bash") => FileType::Code { language: "Bash".into() },
        Some("md") => FileType::Markdown,
        Some("png")
        | Some("jpg")
        | Some("jpeg")
        | Some("gif")
        | Some("svg")
        | Some("webp")
        | Some("bmp") => FileType::Image,
        Some("txt") | Some("log") | Some("csv") => FileType::Text,
        // Unknown extension — try to read as UTF-8 text.
        _ => FileType::Text,
    }
}

// ---------------------------------------------------------------------------
// Content loading
// ---------------------------------------------------------------------------

fn load_content(path: &std::path::Path, file_type: &FileType) -> PreviewContent {
    match file_type {
        FileType::Directory => {
            let mut entries = Vec::new();
            match std::fs::read_dir(path) {
                Ok(iter) => {
                    for entry in iter.flatten() {
                        let name = entry.file_name().to_string_lossy().into_owned();
                        let is_dir = entry.path().is_dir();
                        let size = entry
                            .metadata()
                            .map(|m| if is_dir { 0 } else { m.len() })
                            .unwrap_or(0);
                        entries.push(DirEntry { name, is_dir, size });
                    }
                    // Dirs first, then files; both groups sorted alphabetically.
                    entries.sort_by(|a, b| {
                        b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name))
                    });
                }
                Err(e) => {
                    return PreviewContent::Unsupported(format!(
                        "Cannot read directory: {e}"
                    ));
                }
            }
            PreviewContent::Directory(entries)
        }

        FileType::Image => {
            // Attempt to read basic header metadata; fall back gracefully.
            match image::image_info(path) {
                Some((w, h)) => PreviewContent::ImageInfo { width: w, height: h },
                None => PreviewContent::Unsupported(
                    "Image preview not yet supported.".into(),
                ),
            }
        }

        FileType::Code { language } => {
            match std::fs::read_to_string(path) {
                Ok(src) => {
                    let lines = code::highlight_file(&src, language);
                    PreviewContent::HighlightedCode(lines)
                }
                Err(e) => PreviewContent::Unsupported(format!("Cannot read file: {e}")),
            }
        }

        FileType::Markdown | FileType::Text => match std::fs::read_to_string(path) {
            Ok(text) => PreviewContent::Text(text),
            Err(e) => PreviewContent::Unsupported(format!("Cannot read file: {e}")),
        },

        FileType::Unsupported => {
            // Try UTF-8 as a last resort.
            match std::fs::read_to_string(path) {
                Ok(text) => PreviewContent::Text(text),
                Err(_) => PreviewContent::Unsupported(
                    "Binary or unsupported file format.".into(),
                ),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// PreviewState impl
// ---------------------------------------------------------------------------

impl PreviewState {
    /// Open a file or directory at `path` and load its content.
    pub fn open(path: impl AsRef<std::path::Path>) -> Self {
        let path = path.as_ref().to_path_buf();
        let file_type = detect_file_type(&path);
        let content = load_content(&path, &file_type);
        PreviewState {
            path,
            file_type,
            content,
            scroll_offset: 0,
        }
    }

    /// Navigate to a new path, resetting scroll.
    pub fn navigate_to(&mut self, path: impl AsRef<std::path::Path>) {
        let path = path.as_ref().to_path_buf();
        let file_type = detect_file_type(&path);
        let content = load_content(&path, &file_type);
        self.path = path;
        self.file_type = file_type;
        self.content = content;
        self.scroll_offset = 0;
    }

    /// Return the parent directory of the currently previewed path, if any.
    pub fn parent_dir(&self) -> Option<std::path::PathBuf> {
        self.path.parent().map(|p| p.to_path_buf())
    }
}
