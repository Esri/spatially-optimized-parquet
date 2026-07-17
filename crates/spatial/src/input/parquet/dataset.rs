//! Discovers local Parquet datasets and loads file-qualified footer metadata.

use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use parquet::arrow::arrow_reader::{ArrowReaderMetadata, ArrowReaderOptions};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParquetDataset {
  root: PathBuf,
  files: Vec<ParquetDatasetFile>,
}

impl ParquetDataset {
  /// Discover one local Parquet file or a sorted local Parquet dataset.
  ///
  /// Returns `None` for unsupported paths so another input provider can attempt them.
  pub(crate) fn discover(input: &Path, mode: DiscoveryMode) -> Result<Option<Self>> {
    if input.is_file() {
      if !Self::has_parquet_extension(input) {
        return Ok(None);
      }
      let path = Self::to_absolute_path(input)?;
      let relative_path = path
        .file_name()
        .map(PathBuf::from)
        .context("parquet file path is missing a file name")?;
      return Ok(Some(Self {
        root: input.to_path_buf(),
        files: vec![ParquetDatasetFile {
          path,
          relative_path,
          partition: None,
        }],
      }));
    }
    if !input.is_dir() {
      return Ok(None);
    }

    let root = Self::to_absolute_path(input)?;
    let mut paths = Vec::new();
    Self::discover_directory_files(&root, mode, &mut paths)?;
    if paths.is_empty() {
      bail!("no parquet files found at: {}", input.display());
    }
    paths.sort();
    let files = paths
      .into_iter()
      .map(|path| {
        let relative_path = path
          .strip_prefix(&root)
          .with_context(|| format!("resolve relative parquet path: {}", path.display()))?
          .to_path_buf();
        let partition = PartitionDescriptor::parse(&relative_path)?;
        Ok(ParquetDatasetFile {
          path,
          relative_path,
          partition,
        })
      })
      .collect::<Result<Vec<_>>>()?;

    Ok(Some(Self {
      root: input.to_path_buf(),
      files,
    }))
  }

  /// Return the input path that established this dataset boundary.
  pub(crate) fn root(&self) -> &Path {
    &self.root
  }

  /// Return the discovered Parquet files in deterministic path order.
  pub(crate) fn files(&self) -> &[ParquetDatasetFile] {
    &self.files
  }

  fn discover_directory_files(
    directory: &Path,
    mode: DiscoveryMode,
    files: &mut Vec<PathBuf>,
  ) -> Result<()> {
    for entry in
      fs::read_dir(directory).with_context(|| format!("read directory: {}", directory.display()))?
    {
      let entry = entry?;
      let file_type = entry.file_type()?;
      if file_type.is_symlink() {
        continue;
      }
      let path = entry.path();
      if file_type.is_file() && Self::has_parquet_extension(&path) {
        files.push(path);
      } else if file_type.is_dir() && mode == DiscoveryMode::Recursive {
        Self::discover_directory_files(&path, mode, files)?;
      }
    }
    Ok(())
  }

  fn has_parquet_extension(path: &Path) -> bool {
    path
      .extension()
      .is_some_and(|extension| extension.eq_ignore_ascii_case("parquet"))
  }

  fn to_absolute_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
      Ok(path.to_path_buf())
    } else {
      Ok(
        std::env::current_dir()
          .context("resolve current directory")?
          .join(path),
      )
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiscoveryMode {
  Flat,
  Recursive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PartitionFamily {
  Z,
  Xz,
}

impl PartitionFamily {
  pub(crate) const fn directory_prefix(self) -> &'static str {
    match self {
      Self::Z => "z_order",
      Self::Xz => "xz_order",
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PartitionDescriptor {
  pub(crate) family: PartitionFamily,
  pub(crate) lower_bound: u64,
}

impl PartitionDescriptor {
  pub(crate) fn parse(relative_path: &Path) -> Result<Option<Self>> {
    let mut descriptor = None;
    for component in relative_path.components() {
      let Component::Normal(component) = component else {
        continue;
      };
      let Some(component) = component.to_str() else {
        continue;
      };
      let parsed = Self::parse_component(component)?;
      if parsed.is_some() && descriptor.is_some() {
        bail!(
          "multiple clustering partition directories in {}",
          relative_path.display()
        );
      }
      descriptor = descriptor.or(parsed);
    }
    Ok(descriptor)
  }

  fn parse_component(component: &str) -> Result<Option<Self>> {
    for family in [PartitionFamily::Z, PartitionFamily::Xz] {
      let prefix = format!("{}=", family.directory_prefix());
      if let Some(value) = component.strip_prefix(&prefix) {
        let lower_bound = value
          .parse::<u64>()
          .with_context(|| format!("invalid clustering partition lower bound: {component}"))?;
        return Ok(Some(Self {
          family,
          lower_bound,
        }));
      }
    }
    Ok(None)
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParquetDatasetFile {
  pub(crate) path: PathBuf,
  pub(crate) relative_path: PathBuf,
  pub(crate) partition: Option<PartitionDescriptor>,
}

impl ParquetDatasetFile {
  /// Load Arrow and Parquet metadata from this local file footer.
  pub(crate) fn load_metadata(&self) -> Result<ArrowReaderMetadata> {
    ArrowReaderMetadata::load(
      &fs::File::open(&self.path)
        .with_context(|| format!("open parquet file: {}", self.path.display()))?,
      ArrowReaderOptions::new(),
    )
    .with_context(|| format!("read parquet footer: {}", self.path.display()))
  }
}
