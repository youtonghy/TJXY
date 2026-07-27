use std::str::FromStr;

use tjxy_common::ImageType;

#[test]
fn jellyfin_image_types_parse_exactly_and_reject_path_like_values() {
    for value in [
        "Art",
        "Backdrop",
        "Banner",
        "Box",
        "BoxRear",
        "Chapter",
        "Disc",
        "Logo",
        "Menu",
        "Primary",
        "Profile",
        "Screenshot",
        "Thumb",
    ] {
        let image_type = ImageType::from_str(value).unwrap();
        assert_eq!(image_type.as_str(), value);
    }

    for value in ["primary", "../Primary", "Primary/../../secret", ""] {
        assert!(ImageType::from_str(value).is_err(), "accepted {value:?}");
    }
}
