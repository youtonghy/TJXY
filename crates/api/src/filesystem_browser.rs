use serde::Serialize;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct FilesystemRootDto {
    id: Uuid,
    name: String,
}

impl FilesystemRootDto {
    #[must_use]
    pub fn new(id: Uuid, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct FilesystemDirectoryPageDto {
    items: Vec<FilesystemDirectoryEntryDto>,
}

impl FilesystemDirectoryPageDto {
    #[must_use]
    pub const fn new(items: Vec<FilesystemDirectoryEntryDto>) -> Self {
        Self { items }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct FilesystemDirectoryEntryDto {
    name: String,
    relative_path: String,
    modified_at: Option<String>,
}

impl FilesystemDirectoryEntryDto {
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        relative_path: impl Into<String>,
        modified_at: Option<impl Into<String>>,
    ) -> Self {
        Self {
            name: name.into(),
            relative_path: relative_path.into(),
            modified_at: modified_at.map(Into::into),
        }
    }
}
