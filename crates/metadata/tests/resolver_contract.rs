use std::sync::Arc;

use async_trait::async_trait;
use tjxy_metadata::{
    MetadataItemKind, MetadataLookup, MetadataProvider, MetadataProviderError, MetadataResolver,
    MetadataState,
};

struct UnavailableProvider;
struct InvalidProvider;
struct CompleteProvider;

#[async_trait]
impl MetadataProvider for UnavailableProvider {
    fn name(&self) -> &'static str {
        "Tmdb"
    }

    async fn resolve(
        &self,
        _lookup: &MetadataLookup,
    ) -> Result<Option<tjxy_metadata::MetadataCandidate>, MetadataProviderError> {
        Err(MetadataProviderError::TemporarilyUnavailable)
    }
}

#[async_trait]
impl MetadataProvider for InvalidProvider {
    fn name(&self) -> &'static str {
        "InvalidFixture"
    }

    async fn resolve(
        &self,
        _lookup: &MetadataLookup,
    ) -> Result<Option<tjxy_metadata::MetadataCandidate>, MetadataProviderError> {
        let source =
            tjxy_metadata::MetadataSource::new("InvalidFixture", Option::<String>::None, 5_000)
                .unwrap();
        Ok(Some(
            tjxy_metadata::MetadataCandidate::new(source)
                .with_title("x".repeat(513))
                .with_year(20_000),
        ))
    }
}

#[async_trait]
impl MetadataProvider for CompleteProvider {
    fn name(&self) -> &'static str {
        "Tmdb"
    }

    async fn resolve(
        &self,
        _lookup: &MetadataLookup,
    ) -> Result<Option<tjxy_metadata::MetadataCandidate>, MetadataProviderError> {
        let source =
            tjxy_metadata::MetadataSource::new("Tmdb", Some("movie:329865"), 8_000).unwrap();
        Ok(Some(
            tjxy_metadata::MetadataCandidate::new(source)
                .with_title("Remote Title")
                .with_year(2016)
                .with_overview("Remote overview")
                .with_provider_id("tmdb", "329865"),
        ))
    }
}

#[tokio::test]
async fn provider_failure_keeps_fallback_metadata_and_degrades_to_partial() {
    let lookup = MetadataLookup::new(MetadataItemKind::Movie, "Arrival", Some(2016)).unwrap();
    let resolver = MetadataResolver::new(vec![Arc::new(UnavailableProvider)]).unwrap();

    let resolution = resolver.resolve(&lookup).await;

    assert_eq!(resolution.title(), "Arrival");
    assert_eq!(resolution.production_year(), Some(2016));
    assert_eq!(resolution.overview(), None);
    assert_eq!(resolution.state(), MetadataState::Partial);
    assert_eq!(resolution.warnings().len(), 1);
    assert_eq!(resolution.warnings()[0].provider(), "Tmdb");
    assert_eq!(
        resolution.warnings()[0].error(),
        MetadataProviderError::TemporarilyUnavailable
    );
    assert_eq!(resolution.provenance("title").unwrap().provider(), "Naming");
    assert_eq!(
        resolution.provenance("production_year").unwrap().provider(),
        "Naming"
    );
}

#[tokio::test]
async fn invalid_provider_candidate_is_ignored_and_reported() {
    let lookup = MetadataLookup::new(MetadataItemKind::Movie, "Fallback", Some(2020)).unwrap();
    let resolver = MetadataResolver::new(vec![Arc::new(InvalidProvider)]).unwrap();

    let resolution = resolver.resolve(&lookup).await;

    assert_eq!(resolution.title(), "Fallback");
    assert_eq!(resolution.production_year(), Some(2020));
    assert_eq!(resolution.state(), MetadataState::Partial);
    assert_eq!(resolution.warnings().len(), 1);
    assert_eq!(
        resolution.warnings()[0].error(),
        MetadataProviderError::InvalidResponse
    );
}

#[tokio::test]
async fn initial_sidecar_candidate_precedes_providers_which_fill_missing_fields() {
    let lookup = MetadataLookup::new(MetadataItemKind::Movie, "Fallback", Some(2015)).unwrap();
    let resolver = MetadataResolver::new(vec![Arc::new(CompleteProvider)]).unwrap();
    let source =
        tjxy_metadata::MetadataSource::new("Nfo", Some("storage-object:nfo"), 9_000).unwrap();
    let sidecar = tjxy_metadata::MetadataCandidate::new(source).with_title("Sidecar Title");

    let resolution = resolver
        .resolve_with_candidate(&lookup, sidecar)
        .await
        .unwrap();

    assert_eq!(resolution.title(), "Sidecar Title");
    assert_eq!(resolution.production_year(), Some(2016));
    assert_eq!(resolution.overview(), Some("Remote overview"));
    assert_eq!(resolution.provider_ids().get("tmdb").unwrap(), "329865");
    assert_eq!(resolution.state(), MetadataState::Ready);
    assert_eq!(resolution.provenance("title").unwrap().provider(), "Nfo");
    assert_eq!(
        resolution.provenance("overview").unwrap().provider(),
        "Tmdb"
    );
}
