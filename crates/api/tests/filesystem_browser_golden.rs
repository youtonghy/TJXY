use serde_json::json;
use tjxy_api::{
    AddVirtualFolderDto, AttachVirtualFolderPathDto, FilesystemDirectoryEntryDto,
    FilesystemDirectoryPageDto, FilesystemRootDto,
};
use uuid::Uuid;

#[test]
fn filesystem_browser_dtos_expose_only_opaque_ids_and_relative_paths() {
    let root_id = Uuid::parse_str("018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11").unwrap();
    let root = FilesystemRootDto::new(root_id, "Media");
    let page = FilesystemDirectoryPageDto::new(vec![FilesystemDirectoryEntryDto::new(
        "Movies",
        "Movies",
        Some("2026-08-01T10:00:00Z"),
    )]);

    assert_eq!(
        serde_json::to_value(root).unwrap(),
        json!({"Id": root_id, "Name": "Media"})
    );
    assert_eq!(
        serde_json::to_value(page).unwrap(),
        json!({"Items": [{
            "Name": "Movies",
            "RelativePath": "Movies",
            "ModifiedAt": "2026-08-01T10:00:00Z"
        }]})
    );
}

#[test]
fn library_folder_selections_deserialize_as_opaque_root_and_relative_path() {
    let root_id = Uuid::new_v4();
    let library_id = Uuid::new_v4();
    let create: AddVirtualFolderDto = serde_json::from_value(serde_json::json!({
        "FilesystemSelection": {"RootId": root_id, "RelativePath": "Movies"}
    }))
    .unwrap();
    let create_selection = create.filesystem_selection().unwrap();
    assert_eq!(create_selection.root_id(), root_id);
    assert_eq!(create_selection.relative_path(), "Movies");

    let attach: AttachVirtualFolderPathDto = serde_json::from_value(serde_json::json!({
        "LibraryId": library_id,
        "FilesystemSelection": {"RootId": root_id, "RelativePath": "TV"}
    }))
    .unwrap();
    assert_eq!(attach.library_id(), library_id);
    let attach_selection = attach.filesystem_selection().unwrap();
    assert_eq!(attach_selection.root_id(), root_id);
    assert_eq!(attach_selection.relative_path(), "TV");
}
