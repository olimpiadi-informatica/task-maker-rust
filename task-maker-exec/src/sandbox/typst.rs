use anyhow::{anyhow, Context, Error};
use reqwest::blocking::{Client, ClientBuilder};
use tar::Archive;
use task_maker_dag::{Execution, FileUuid};
use task_maker_store::FileStoreHandle;
use typst::ecow::eco_format;
use typst::syntax::package::PackageSpec;
use typst_pdf::PdfOptions;
use zune_inflate::DeflateDecoder;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use std::{env, fs};
use std::path::{Path, PathBuf};

use typst::diag::{FileError, FileResult, PackageError, PackageResult};
use typst::foundations::{Bytes, Datetime, Dict, Str, Value};
use typst::syntax::{FileId, Source, VirtualPath};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, World};
use typst_kit::fonts::{FontSearcher, FontSlot};

use crate::sandbox::SandboxResult;

#[derive(Debug, Clone)]
pub struct TypstCompiler {
    root: PathBuf,
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Arc<Vec<FontSlot>>,
    main: FileId,
    cache_dir: PathBuf,
    http_client: Client,
    files: HashMap<PathBuf, PathBuf>,
    outputs: HashMap<PathBuf, Vec<u8>>,
}

impl TypstCompiler {
    pub fn new(root: &Path, execution: &Execution, dep_keys: &HashMap<FileUuid, FileStoreHandle>) -> anyhow::Result<TypstCompiler> {
        let files = execution.inputs.iter().map(|(path, input)| Ok::<_, anyhow::Error>((
            path.strip_prefix("./").unwrap_or(path).to_owned(), 
            dep_keys
                .get(&input.file)
                .context("file not provided")?
                .path()
                .to_owned(),
        )))
        .collect::<Result<HashMap<_, _>, _>>()?;
        
        let fonts = FontSearcher::new().include_system_fonts(true).search();

        let cache_dir = match env::var("XDG_CACHE_HOME") {
            Ok(cache) => Path::new(&cache).join("typst/packages"),
            Err(_) => Path::new(&env::var("HOME")?).join(".cache/typst/packages"),
        };

        let mut inputs = Dict::new();
        inputs.insert(Str::from("gen_gen"), Value::Str(Str::from("GEN")));
        inputs.insert(Str::from("constraints_yaml"), Value::Str(Str::from("constraints.yaml")));
        inputs.insert(Str::from("contest_yaml"), Value::Str(Str::from("../../contest.yaml")));

        let library = Library::builder()
            .with_inputs(inputs)
            .build();

        let http_client = ClientBuilder::new()
            .timeout(Duration::from_secs(10))
            .build()?;

        Ok(TypstCompiler {
            root: root.to_owned(),
            library: LazyHash::new(library),
            book: LazyHash::new(fonts.book),
            fonts: Arc::new(fonts.fonts),
            main: FileId::new(None, VirtualPath::new("booklet.typ")),
            cache_dir,
            http_client,
            files,
            outputs: HashMap::new(),
        })
    }

    /// Compile the Typst file
    pub fn run(&mut self) -> Result<SandboxResult, Error> {
        let document = typst::compile(self)
            .output
            .map_err(|err| anyhow!("Error compiling typst: {err:?}"))?;
        let pdf = typst_pdf::pdf(&document, &PdfOptions::default())
            .map_err(|err| anyhow!("Error compiling typst: {err:?}"))?;

        self.outputs.insert(Path::new("booklet.pdf").to_owned(), pdf);

        Ok(SandboxResult::default())
    }

    pub fn output(&self, path: &Path) -> Vec<u8> {
        self.outputs.get(path).unwrap_or(&vec![]).clone()
    }

    fn get_package_dir(&self, package: &PackageSpec) -> PackageResult<PathBuf> {
        let PackageSpec { namespace, name, version } = package;
        let package_subdir = format!("{namespace}/{name}/{version}");
        let path = self.cache_dir.join(package_subdir);

        if !path.exists() {
            let url = format!("https://packages.typst.org/{namespace}/{name}-{version}.tar.gz");
            let req = self.http_client
                .get(url)
                .send()
                .map_err(|err| PackageError::NetworkFailed(Some(eco_format!("{err}"))))?
                .error_for_status()
                .map_err(|err| PackageError::NetworkFailed(Some(eco_format!("{err}"))))?
                .bytes()    
                .map_err(|err| PackageError::NetworkFailed(Some(eco_format!("{err}"))))?;

            let archive = DeflateDecoder::new(&req)
                .decode_gzip()
                .map_err(|err| PackageError::MalformedArchive(Some(eco_format!("{err}"))))?;

            let mut archive = Archive::new(archive.as_slice());
            archive.unpack(&path).map_err(|err| {
                _ = fs::remove_dir_all(&path);
                PackageError::MalformedArchive(Some(eco_format!("{err}")))
            })?;
        }

        Ok(path)
    }

    fn resolve_path(&self, id: FileId) -> FileResult<PathBuf> {
        let path = if let Some(package) = id.package() {
            let package_dir = self.get_package_dir(package)?;
            id
                .vpath()
                .resolve(&package_dir)
                .ok_or(FileError::AccessDenied)?
                .clone()
        } else {
            let path = id.vpath().resolve(&self.root)
                .ok_or(FileError::AccessDenied)?;
            let path = path
                .strip_prefix("./")
                .unwrap_or(&path);
            self.files[path].clone()
        };

        Ok(path)
    }
}

impl World for TypstCompiler {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) ->  &LazyHash<FontBook> {
        &self.book
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts[index].get()
    }

    fn main(&self) -> FileId {
        self.main
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        let path = self.resolve_path(id)?;

        let bytes = fs::read(&path).map_err(|err| FileError::from_io(err, &path))?;
        let contents = std::str::from_utf8(&bytes)
            .map_err(|_| FileError::InvalidUtf8)?
            .trim_start_matches('\u{feff}');

        Ok(Source::new(id, contents.to_owned()))
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        let path = self.resolve_path(id)?;

        match fs::read(&path) {
            Ok(file) => Ok(Bytes::new(file).clone()),
            Err(err) => Err(FileError::from_io(err, &path)),
        }
    }

    fn today(&self, offset: Option<i64>) -> Option<Datetime> {
        let offset = offset.unwrap_or(0).try_into().ok()?;
        let offset = time::UtcOffset::from_hms(offset, 0, 0).ok()?;
        time::OffsetDateTime::now_utc()
            .checked_to_offset(offset)
            .map(|time| Datetime::Date(time.date()))
    }
}

