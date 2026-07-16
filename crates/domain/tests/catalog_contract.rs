use tjxy_common::{CatalogItemId, MediaSourceId, PresentationKey};
use tjxy_domain::{
    CatalogItem, CatalogItemKind, EffectiveScanPolicy, MediaLocation, MediaSource, MetadataPolicy,
    ObjectSelectionScope, PresenceState, ProbePolicy, ScanProfile, StructureExpansionPolicy,
};

#[test]
fn catalog_identity_and_presentation_key_do_not_depend_on_paths() {
    let item = CatalogItem::new(CatalogItemKind::Movie, "Arrival");
    let source = MediaSource::new(item.id());
    let presentation_key = source.presentation_key();

    let source = source.with_location(MediaLocation::new("filesystem-object-1"));

    assert_eq!(source.catalog_item_id(), item.id());
    assert_eq!(source.presentation_key(), presentation_key);
    assert!(!presentation_key.to_string().contains("filesystem-object-1"));
}

#[test]
fn externally_visible_ids_are_lowercase_hyphenated_uuids() {
    for value in [
        CatalogItemId::new().to_string(),
        MediaSourceId::new().to_string(),
        PresentationKey::new().to_string(),
    ] {
        assert_eq!(value.len(), 36);
        assert_eq!(value, value.to_lowercase());
        assert_eq!(uuid::Uuid::parse_str(&value).unwrap().to_string(), value);
    }
}

#[test]
fn scan_profiles_expand_to_persistable_effective_policies() {
    let full = EffectiveScanPolicy::for_profile(ScanProfile::Full);
    assert_eq!(full.object_selection, ObjectSelectionScope::EntireRoot);
    assert_eq!(full.metadata, MetadataPolicy::Full);
    assert_eq!(full.expansion, StructureExpansionPolicy::Eager);
    assert_eq!(full.probe, ProbePolicy::Eager);

    let lazy = EffectiveScanPolicy::for_profile(ScanProfile::Lazy);
    assert_eq!(lazy.object_selection, ObjectSelectionScope::OnDemandSubtree);
    assert_eq!(lazy.metadata, MetadataPolicy::Basic);
    assert_eq!(lazy.expansion, StructureExpansionPolicy::OnAccess);
    assert_eq!(lazy.probe, ProbePolicy::OnPlaybackInfo);

    let hybrid = EffectiveScanPolicy::for_profile(ScanProfile::Hybrid);
    assert_eq!(hybrid.object_selection, ObjectSelectionScope::EntireRoot);
    assert_eq!(hybrid.metadata, MetadataPolicy::Basic);
    assert_eq!(hybrid.expansion, StructureExpansionPolicy::Background);
    assert_eq!(hybrid.probe, ProbePolicy::OnPlaybackInfo);

    let manual = EffectiveScanPolicy::for_profile(ScanProfile::Manual);
    assert_eq!(manual.object_selection, ObjectSelectionScope::ExplicitOnly);
    assert_eq!(manual.metadata, MetadataPolicy::ExplicitOnly);
    assert_eq!(manual.expansion, StructureExpansionPolicy::ExplicitOnly);
    assert_eq!(manual.probe, ProbePolicy::ExplicitOnly);
}

#[test]
fn only_confirmed_absence_can_detach_catalog_locations() {
    assert!(!PresenceState::Present.allows_detach());
    assert!(!PresenceState::TemporarilyUnavailable.allows_detach());
    assert!(PresenceState::ConfirmedAbsent.allows_detach());
}
