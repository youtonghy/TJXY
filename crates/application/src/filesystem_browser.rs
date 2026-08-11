use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
};

use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

const MAX_DIRECTORY_ENTRIES: usize = 10_000;

#[derive(Clone)]
pub struct FilesystemBrowser {
    roots: Vec<ConfiguredRoot>,
}

#[derive(Clone)]
struct ConfiguredRoot {
    id: Uuid,
    label: String,
    canonical_path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilesystemBrowserRoot {
    id: Uuid,
    label: String,
}

impl FilesystemBrowserRoot {
    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }

    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilesystemDirectoryPage {
    entries: Vec<FilesystemDirectoryEntry>,
}

impl FilesystemDirectoryPage {
    #[must_use]
    pub fn entries(&self) -> &[FilesystemDirectoryEntry] {
        &self.entries
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilesystemDirectoryEntry {
    name: String,
    relative_path: String,
    modified_at: Option<DateTime<Utc>>,
}

impl FilesystemDirectoryEntry {
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }

    #[must_use]
    pub const fn modified_at(&self) -> Option<DateTime<Utc>> {
        self.modified_at
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedFilesystemDirectory {
    path: PathBuf,
}

impl ResolvedFilesystemDirectory {
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl FilesystemBrowser {
    /// Builds a browser from canonical, readable server directories.
    ///
    /// # Errors
    ///
    /// Returns a sanitized configuration error for missing, duplicate, or invalid roots.
    pub async fn from_roots<I, P>(roots: I) -> Result<Self, FilesystemBrowserError>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let mut configured = Vec::new();
        let mut seen = HashSet::new();
        for (index, root) in roots.into_iter().enumerate() {
            configured.push(configure_root(index, root.as_ref(), &mut seen).await?);
        }
        Ok(Self { roots: configured })
    }

    /// Builds a browser from every currently available root and reports skipped indexes.
    ///
    /// Missing, unreadable, non-directory, and duplicate roots are omitted so persisted
    /// configuration cannot prevent the rest of the application from starting.
    pub async fn from_available_roots<I, P>(roots: I) -> (Option<Self>, Vec<usize>)
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let mut configured = Vec::new();
        let mut invalid_root_indexes = Vec::new();
        let mut seen = HashSet::new();
        for (index, root) in roots.into_iter().enumerate() {
            match configure_root(index, root.as_ref(), &mut seen).await {
                Ok(root) => configured.push(root),
                Err(
                    FilesystemBrowserError::InvalidRoot { .. }
                    | FilesystemBrowserError::DuplicateRoot { .. },
                ) => invalid_root_indexes.push(index),
                Err(_) => unreachable!("root configuration only returns root errors"),
            }
        }
        let browser = (!configured.is_empty()).then_some(Self { roots: configured });
        (browser, invalid_root_indexes)
    }

    #[must_use]
    pub fn roots(&self) -> Vec<FilesystemBrowserRoot> {
        self.roots
            .iter()
            .map(|root| FilesystemBrowserRoot {
                id: root.id,
                label: root.label.clone(),
            })
            .collect()
    }

    /// Lists direct child directories under one validated selection.
    ///
    /// # Errors
    ///
    /// Returns a sanitized error for invalid paths, escaped selections, or unavailable directories.
    pub async fn list(
        &self,
        root_id: Uuid,
        relative_path: &Path,
    ) -> Result<FilesystemDirectoryPage, FilesystemBrowserError> {
        let resolved = self.resolve(root_id, relative_path).await?;
        let root = self.root(root_id)?;
        let mut reader = tokio::fs::read_dir(resolved.path())
            .await
            .map_err(|_| FilesystemBrowserError::DirectoryUnavailable)?;
        let mut entries = Vec::new();
        while let Some(entry) = reader
            .next_entry()
            .await
            .map_err(|_| FilesystemBrowserError::DirectoryUnavailable)?
        {
            if entries.len() >= MAX_DIRECTORY_ENTRIES {
                return Err(FilesystemBrowserError::DirectoryLimit);
            }
            let file_type = entry
                .file_type()
                .await
                .map_err(|_| FilesystemBrowserError::DirectoryUnavailable)?;
            if !file_type.is_dir() {
                continue;
            }
            let canonical = tokio::fs::canonicalize(entry.path())
                .await
                .map_err(|_| FilesystemBrowserError::DirectoryUnavailable)?;
            if !canonical.starts_with(&root.canonical_path) {
                continue;
            }
            let relative = canonical
                .strip_prefix(&root.canonical_path)
                .map_err(|_| FilesystemBrowserError::EscapedRoot)?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| FilesystemBrowserError::InvalidDirectoryName)?;
            let relative_path = relative
                .to_str()
                .ok_or(FilesystemBrowserError::InvalidDirectoryName)?
                .to_owned();
            let modified_at = entry
                .metadata()
                .await
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .map(DateTime::<Utc>::from);
            entries.push(FilesystemDirectoryEntry {
                name,
                relative_path,
                modified_at,
            });
        }
        entries.sort_by(|left, right| {
            left.name
                .to_lowercase()
                .cmp(&right.name.to_lowercase())
                .then_with(|| left.name.cmp(&right.name))
        });
        Ok(FilesystemDirectoryPage { entries })
    }

    /// Resolves a browser selection to an internal canonical directory.
    ///
    /// # Errors
    ///
    /// Returns a sanitized error if the selection is invalid, stale, or outside the allowed root.
    pub async fn resolve(
        &self,
        root_id: Uuid,
        relative_path: &Path,
    ) -> Result<ResolvedFilesystemDirectory, FilesystemBrowserError> {
        validate_relative_path(relative_path)?;
        let root = self.root(root_id)?;
        let path = tokio::fs::canonicalize(root.canonical_path.join(relative_path))
            .await
            .map_err(|_| FilesystemBrowserError::DirectoryUnavailable)?;
        if !path.starts_with(&root.canonical_path) {
            return Err(FilesystemBrowserError::EscapedRoot);
        }
        if !tokio::fs::metadata(&path)
            .await
            .map_err(|_| FilesystemBrowserError::DirectoryUnavailable)?
            .is_dir()
        {
            return Err(FilesystemBrowserError::DirectoryUnavailable);
        }
        Ok(ResolvedFilesystemDirectory { path })
    }

    fn root(&self, root_id: Uuid) -> Result<&ConfiguredRoot, FilesystemBrowserError> {
        self.roots
            .iter()
            .find(|root| root.id == root_id)
            .ok_or(FilesystemBrowserError::UnknownRoot)
    }
}

async fn configure_root(
    index: usize,
    root: &Path,
    seen: &mut HashSet<PathBuf>,
) -> Result<ConfiguredRoot, FilesystemBrowserError> {
    let canonical_path = tokio::fs::canonicalize(root)
        .await
        .map_err(|_| FilesystemBrowserError::InvalidRoot { index })?;
    let metadata = tokio::fs::metadata(&canonical_path)
        .await
        .map_err(|_| FilesystemBrowserError::InvalidRoot { index })?;
    if !metadata.is_dir() {
        return Err(FilesystemBrowserError::InvalidRoot { index });
    }
    if !seen.insert(canonical_path.clone()) {
        return Err(FilesystemBrowserError::DuplicateRoot { index });
    }
    let label = canonical_path
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map_or_else(|| format!("Media root {}", index + 1), str::to_owned);
    let id = Uuid::new_v5(
        &Uuid::NAMESPACE_OID,
        canonical_path.as_os_str().as_encoded_bytes(),
    );
    Ok(ConfiguredRoot {
        id,
        label,
        canonical_path,
    })
}

fn validate_relative_path(path: &Path) -> Result<(), FilesystemBrowserError> {
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(FilesystemBrowserError::InvalidRelativePath);
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum FilesystemBrowserError {
    #[error("filesystem browser root {index} is invalid")]
    InvalidRoot { index: usize },
    #[error("filesystem browser root {index} duplicates an earlier root")]
    DuplicateRoot { index: usize },
    #[error("filesystem browser root is unavailable")]
    UnknownRoot,
    #[error("filesystem browser path must be relative and normalized")]
    InvalidRelativePath,
    #[error("filesystem browser selection escaped its allowed root")]
    EscapedRoot,
    #[error("filesystem browser directory is unavailable")]
    DirectoryUnavailable,
    #[error("filesystem browser directory name is not supported")]
    InvalidDirectoryName,
    #[error("filesystem browser directory exceeds the entry limit")]
    DirectoryLimit,
}
