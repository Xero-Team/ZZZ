use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};

use anyhow::{Context as _, Result};
use gpui::{RenderImage, SvgRenderer};
use parking_lot::Mutex;
use typst::diag::{FileError, FileResult, PackageError, SourceDiagnostic, Warned};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::package::PackageSpec;
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World, WorldExt};
use typst_kit::datetime::Time;
use typst_kit::downloader::SystemDownloader;
use typst_kit::files::{FileLoader, FileStore, FsRoot};
use typst_kit::fonts::{self, FontStore};
use typst_kit::packages::{FsPackages, SystemPackages, UniversePackages};
use typst_layout::PagedDocument;
use typst_svg::SvgOptions;

static LIBRARY: LazyLock<LazyHash<Library>> =
    LazyLock::new(|| LazyHash::new(Library::builder().build()));
static FONTS: LazyLock<FontStore> = LazyLock::new(|| {
    let mut store = FontStore::new();
    store.extend(fonts::system());
    store.extend(fonts::embedded());
    store
});

struct PreviewFileLoader {
    project_files: FsRoot,
    package_data: Option<FsPackages>,
    package_cache: Option<FsPackages>,
    remote: bool,
    remote_files: HashMap<VirtualPath, Bytes>,
    missing_remote_files: Mutex<HashSet<VirtualPath>>,
    missing_packages: Mutex<HashSet<PackageSpec>>,
}

impl PreviewFileLoader {
    fn new(root: PathBuf, remote: bool) -> Self {
        Self {
            project_files: FsRoot::new(root),
            package_data: FsPackages::system_data(),
            package_cache: FsPackages::system_cache(),
            remote,
            remote_files: HashMap::new(),
            missing_remote_files: Mutex::new(HashSet::new()),
            missing_packages: Mutex::new(HashSet::new()),
        }
    }

    fn clear_remote_files(&mut self) {
        if self.remote {
            self.remote_files.clear();
        }
    }

    fn insert_remote_file(&mut self, path: VirtualPath, bytes: Vec<u8>) {
        self.remote_files.insert(path, Bytes::new(bytes));
    }

    fn begin_compile(&self) {
        self.missing_remote_files.lock().clear();
        self.missing_packages.lock().clear();
    }

    fn take_missing_remote_files(&self) -> Vec<VirtualPath> {
        let mut files = self.missing_remote_files.lock().drain().collect::<Vec<_>>();
        files.sort_by(|left, right| left.get_with_slash().cmp(right.get_with_slash()));
        files
    }

    fn take_missing_packages(&self) -> Vec<PackageSpec> {
        let mut packages = self.missing_packages.lock().drain().collect::<Vec<_>>();
        packages.sort_by_key(ToString::to_string);
        packages
    }
}

impl FileLoader for PreviewFileLoader {
    fn load(&self, id: FileId) -> FileResult<Bytes> {
        match id.root() {
            VirtualRoot::Project if self.remote => {
                if let Some(bytes) = self.remote_files.get(id.vpath()) {
                    return Ok(bytes.clone());
                }

                self.missing_remote_files.lock().insert(id.vpath().clone());
                Err(FileError::NotFound(PathBuf::from(
                    id.vpath().get_without_slash(),
                )))
            }
            VirtualRoot::Project => self.project_files.load(id.vpath()),
            VirtualRoot::Package(package) => {
                let root = self
                    .package_data
                    .as_ref()
                    .and_then(|packages| packages.obtain(package))
                    .or_else(|| {
                        self.package_cache
                            .as_ref()
                            .and_then(|packages| packages.obtain(package))
                    });
                let Some(root) = root else {
                    if package.namespace == UniversePackages::NAMESPACE {
                        self.missing_packages.lock().insert(package.clone());
                    }
                    return Err(FileError::Package(PackageError::NotFound(package.clone())));
                };
                root.load(id.vpath())
            }
        }
    }
}

pub(crate) struct TypstCompiler {
    preview_source: Source,
    files: FileStore<PreviewFileLoader>,
    time: Time,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PreviewColors {
    background: u32,
    foreground: u32,
}

impl PreviewColors {
    pub fn new(background: u32, foreground: u32) -> Self {
        Self {
            background,
            foreground,
        }
    }
}

pub(crate) enum CompileAttempt {
    NeedsRemoteFiles(Vec<VirtualPath>),
    Complete(CompileResult),
}

pub(crate) struct CompileResult {
    pub pages: Result<Vec<Arc<RenderImage>>, String>,
    pub warnings: Option<String>,
    pub missing_packages: Vec<PackageSpec>,
}

impl TypstCompiler {
    pub fn new(
        root: PathBuf,
        main_path: &str,
        remote: bool,
        colors: PreviewColors,
    ) -> Result<Self> {
        let main_path = VirtualPath::new(main_path).context("invalid Typst main file path")?;
        let preview_id = FileId::unique(RootedPath::new(VirtualRoot::Project, main_path.clone()));
        let included_path = escape_typst_string(main_path.get_with_slash());
        let preview_source = Source::new(
            preview_id,
            format!(
                "#set page(fill: rgb(\"#{:06x}\"))\n#set text(fill: rgb(\"#{:06x}\"))\n#include \"{included_path}\"",
                colors.background, colors.foreground
            ),
        );
        Ok(Self {
            preview_source,
            files: FileStore::new(PreviewFileLoader::new(root, remote)),
            time: Time::system(),
        })
    }

    pub fn clear_remote_files(&mut self) {
        self.files.loader_mut().clear_remote_files();
    }

    pub fn insert_remote_file(&mut self, path: VirtualPath, bytes: Vec<u8>) {
        self.files.loader_mut().insert_remote_file(path, bytes);
    }

    pub fn compile(&mut self, renderer: &SvgRenderer) -> CompileAttempt {
        self.files.reset();
        self.time.reset();
        self.files.loader().begin_compile();

        let Warned { output, warnings } = typst::compile::<PagedDocument>(self);
        let missing_remote_files = self.files.loader().take_missing_remote_files();
        if !missing_remote_files.is_empty() {
            return CompileAttempt::NeedsRemoteFiles(missing_remote_files);
        }

        let missing_packages = self.files.loader().take_missing_packages();
        let warnings = (!warnings.is_empty()).then(|| self.format_diagnostics(&warnings));
        let pages = output
            .map_err(|diagnostics| self.format_diagnostics(&diagnostics))
            .and_then(|document| render_pages(&document, renderer));

        CompileAttempt::Complete(CompileResult {
            pages,
            warnings,
            missing_packages,
        })
    }

    fn format_diagnostics(&self, diagnostics: &[SourceDiagnostic]) -> String {
        diagnostics
            .iter()
            .map(|diagnostic| {
                let mut text = self
                    .diagnostic_location(diagnostic)
                    .map_or_else(String::new, |location| format!("{location}: "));
                text.push_str(diagnostic.message.as_str());
                for hint in &diagnostic.hints {
                    text.push_str("\n  ");
                    text.push_str(hint.v.as_str());
                }
                text
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn diagnostic_location(&self, diagnostic: &SourceDiagnostic) -> Option<String> {
        let file_id = diagnostic.span.id()?;
        let range = self.range(diagnostic.span)?;
        let source = self.source(file_id).ok()?;
        let (line, column) = source.lines().byte_to_line_column(range.start)?;
        let path = match file_id.root() {
            VirtualRoot::Project => file_id.vpath().get_without_slash().to_owned(),
            VirtualRoot::Package(package) => {
                format!("{package}{}", file_id.vpath().get_with_slash())
            }
        };
        Some(format!("{}:{}:{}", path, line + 1, column + 1))
    }
}

impl World for TypstCompiler {
    fn library(&self) -> &LazyHash<Library> {
        &LIBRARY
    }

    fn book(&self) -> &LazyHash<FontBook> {
        FONTS.book()
    }

    fn main(&self) -> FileId {
        self.preview_source.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.preview_source.id() {
            Ok(self.preview_source.clone())
        } else {
            self.files.source(id)
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if id == self.preview_source.id() {
            Ok(Bytes::from_string(self.preview_source.clone()))
        } else {
            self.files.file(id)
        }
    }

    fn font(&self, index: usize) -> Option<Font> {
        FONTS.font(index)
    }

    fn today(&self, offset: Option<Duration>) -> Option<Datetime> {
        self.time.today(offset)
    }
}

fn escape_typst_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

pub(crate) fn download_package(package: &PackageSpec) -> Result<()> {
    SystemPackages::new(SystemDownloader::new("ZZZ Typst Preview"))
        .obtain(package)
        .with_context(|| format!("failed to download Typst package {package}"))?;
    Ok(())
}

fn render_pages(
    document: &PagedDocument,
    renderer: &SvgRenderer,
) -> Result<Vec<Arc<RenderImage>>, String> {
    document
        .pages()
        .iter()
        .map(|page| {
            let svg = typst_svg::svg(page, &SvgOptions::default());
            renderer
                .render_single_frame(svg.as_bytes(), 1.0)
                .map_err(|error| error.to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIGHT_COLORS: PreviewColors = PreviewColors {
        background: 0xffffff,
        foreground: 0x000000,
    };
    const DARK_COLORS: PreviewColors = PreviewColors {
        background: 0x202020,
        foreground: 0xf0f0f0,
    };

    #[test]
    fn compiles_multiple_pages() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let main_path = directory.path().join("main.typ");
        std::fs::write(&main_path, "First page #pagebreak() Second page")?;

        let mut compiler = TypstCompiler::new(
            directory.path().to_path_buf(),
            "main.typ",
            false,
            LIGHT_COLORS,
        )?;
        let renderer = SvgRenderer::new(Arc::new(()));
        let CompileAttempt::Complete(result) = compiler.compile(&renderer) else {
            anyhow::bail!("local compilation requested remote files");
        };
        let pages = result.pages.map_err(anyhow::Error::msg)?;

        assert_eq!(pages.len(), 2);
        Ok(())
    }

    #[test]
    fn compiles_empty_document() -> Result<()> {
        let directory = tempfile::tempdir()?;
        std::fs::write(directory.path().join("main.typ"), "")?;

        let mut compiler = TypstCompiler::new(
            directory.path().to_path_buf(),
            "main.typ",
            false,
            LIGHT_COLORS,
        )?;
        let renderer = SvgRenderer::new(Arc::new(()));
        let CompileAttempt::Complete(result) = compiler.compile(&renderer) else {
            anyhow::bail!("local compilation requested remote files");
        };

        assert_eq!(result.pages.map_err(anyhow::Error::msg)?.len(), 1);
        Ok(())
    }

    #[test]
    fn reads_local_imports_from_disk() -> Result<()> {
        let directory = tempfile::tempdir()?;
        std::fs::write(
            directory.path().join("main.typ"),
            "#include \"chapter.typ\"",
        )?;
        std::fs::write(directory.path().join("chapter.typ"), "Local chapter")?;

        let mut compiler = TypstCompiler::new(
            directory.path().to_path_buf(),
            "main.typ",
            false,
            LIGHT_COLORS,
        )?;
        let renderer = SvgRenderer::new(Arc::new(()));
        let CompileAttempt::Complete(result) = compiler.compile(&renderer) else {
            anyhow::bail!("local compilation requested remote files");
        };

        assert_eq!(result.pages.map_err(anyhow::Error::msg)?.len(), 1);
        Ok(())
    }

    #[test]
    fn recompiles_after_an_error() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let main_path = directory.path().join("main.typ");
        std::fs::write(&main_path, "#let")?;

        let mut compiler = TypstCompiler::new(
            directory.path().to_path_buf(),
            "main.typ",
            false,
            LIGHT_COLORS,
        )?;
        let renderer = SvgRenderer::new(Arc::new(()));
        let CompileAttempt::Complete(result) = compiler.compile(&renderer) else {
            anyhow::bail!("local compilation requested remote files");
        };
        let Err(error) = result.pages else {
            anyhow::bail!("invalid Typst source compiled successfully");
        };
        assert!(
            error.starts_with("main.typ:1:"),
            "unexpected diagnostic: {error}"
        );

        std::fs::write(&main_path, "Recovered")?;
        let CompileAttempt::Complete(result) = compiler.compile(&renderer) else {
            anyhow::bail!("local compilation requested remote files");
        };
        assert_eq!(result.pages.map_err(anyhow::Error::msg)?.len(), 1);
        Ok(())
    }

    #[test]
    fn remote_compilation_reports_files_to_fetch() -> Result<()> {
        let mut compiler =
            TypstCompiler::new(PathBuf::from("/remote"), "main.typ", true, LIGHT_COLORS)?;
        let renderer = SvgRenderer::new(Arc::new(()));
        let CompileAttempt::NeedsRemoteFiles(files) = compiler.compile(&renderer) else {
            anyhow::bail!("remote compilation did not request its main file");
        };

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].get_without_slash(), "main.typ");
        Ok(())
    }

    #[test]
    fn remote_compilation_loads_imports_incrementally() -> Result<()> {
        let mut compiler =
            TypstCompiler::new(PathBuf::from("/remote"), "main.typ", true, LIGHT_COLORS)?;
        let renderer = SvgRenderer::new(Arc::new(()));
        compiler.insert_remote_file(
            VirtualPath::new("main.typ")?,
            b"#include \"chapter.typ\"".to_vec(),
        );

        let CompileAttempt::NeedsRemoteFiles(files) = compiler.compile(&renderer) else {
            anyhow::bail!("remote compilation did not request its import");
        };
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].get_without_slash(), "chapter.typ");

        compiler.insert_remote_file(files[0].clone(), b"Remote chapter".to_vec());
        let CompileAttempt::Complete(result) = compiler.compile(&renderer) else {
            anyhow::bail!("remote compilation requested an already loaded import");
        };
        assert_eq!(result.pages.map_err(anyhow::Error::msg)?.len(), 1);
        Ok(())
    }

    #[test]
    fn missing_package_is_reported_without_downloading() -> Result<()> {
        let directory = tempfile::tempdir()?;
        std::fs::write(
            directory.path().join("main.typ"),
            "#import \"@preview/zzz-missing-package:0.1.0\": *",
        )?;

        let mut compiler = TypstCompiler::new(
            directory.path().to_path_buf(),
            "main.typ",
            false,
            LIGHT_COLORS,
        )?;
        let renderer = SvgRenderer::new(Arc::new(()));
        let CompileAttempt::Complete(result) = compiler.compile(&renderer) else {
            anyhow::bail!("local compilation requested remote files");
        };

        assert_eq!(result.missing_packages.len(), 1);
        assert_eq!(
            result.missing_packages[0].to_string(),
            "@preview/zzz-missing-package:0.1.0"
        );
        assert!(result.pages.is_err());
        Ok(())
    }

    #[test]
    fn applies_preview_theme_defaults() -> Result<()> {
        let directory = tempfile::tempdir()?;
        std::fs::write(directory.path().join("main.typ"), "Themed text")?;
        let renderer = SvgRenderer::new(Arc::new(()));

        let mut light = TypstCompiler::new(
            directory.path().to_path_buf(),
            "main.typ",
            false,
            LIGHT_COLORS,
        )?;
        let CompileAttempt::Complete(light_result) = light.compile(&renderer) else {
            anyhow::bail!("local compilation requested remote files");
        };
        let light_pages = light_result.pages.map_err(anyhow::Error::msg)?;

        let mut dark = TypstCompiler::new(
            directory.path().to_path_buf(),
            "main.typ",
            false,
            DARK_COLORS,
        )?;
        let CompileAttempt::Complete(dark_result) = dark.compile(&renderer) else {
            anyhow::bail!("local compilation requested remote files");
        };
        let dark_pages = dark_result.pages.map_err(anyhow::Error::msg)?;

        assert_ne!(light_pages[0].as_bytes(0), dark_pages[0].as_bytes(0));
        Ok(())
    }

    #[test]
    fn authored_colors_override_preview_theme() -> Result<()> {
        let directory = tempfile::tempdir()?;
        std::fs::write(
            directory.path().join("main.typ"),
            "#set page(fill: white)\n#set text(fill: black)\nAuthored colors",
        )?;
        let renderer = SvgRenderer::new(Arc::new(()));

        let mut light = TypstCompiler::new(
            directory.path().to_path_buf(),
            "main.typ",
            false,
            LIGHT_COLORS,
        )?;
        let CompileAttempt::Complete(light_result) = light.compile(&renderer) else {
            anyhow::bail!("local compilation requested remote files");
        };
        let light_pages = light_result.pages.map_err(anyhow::Error::msg)?;

        let mut dark = TypstCompiler::new(
            directory.path().to_path_buf(),
            "main.typ",
            false,
            DARK_COLORS,
        )?;
        let CompileAttempt::Complete(dark_result) = dark.compile(&renderer) else {
            anyhow::bail!("local compilation requested remote files");
        };
        let dark_pages = dark_result.pages.map_err(anyhow::Error::msg)?;

        assert_eq!(light_pages[0].as_bytes(0), dark_pages[0].as_bytes(0));
        Ok(())
    }
}
