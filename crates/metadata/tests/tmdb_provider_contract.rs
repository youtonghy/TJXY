use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tjxy_metadata::{
    MetadataCandidate, MetadataItemKind, MetadataLookup, MetadataNamedValue, MetadataPerson,
    MetadataProvider, MetadataProviderError, MetadataSource, TmdbProvider, TmdbSearchItem,
    TmdbTransport,
};

type SearchCall = (MetadataItemKind, String, Option<i32>, String);
type DetailCall = (MetadataItemKind, u64, String);

struct FakeTransport {
    calls: Mutex<Vec<SearchCall>>,
    detail_calls: Mutex<Vec<DetailCall>>,
    validation_result: Result<(), MetadataProviderError>,
}

#[async_trait]
impl TmdbTransport for FakeTransport {
    async fn validate(&self) -> Result<(), MetadataProviderError> {
        self.validation_result
    }

    async fn search(
        &self,
        kind: MetadataItemKind,
        query: &str,
        year: Option<i32>,
        language: &str,
    ) -> Result<Vec<TmdbSearchItem>, MetadataProviderError> {
        self.calls
            .lock()
            .unwrap()
            .push((kind, query.to_owned(), year, language.to_owned()));
        Ok(vec![
            TmdbSearchItem::new(1, "Wrong Year").with_details(None, None, Some(2015)),
            TmdbSearchItem::new(329_865, "Arrival")
                .with_details(
                    Some("Arrival".to_owned()),
                    Some("A linguist meets visitors.".to_owned()),
                    Some(2016),
                )
                .with_poster_path("/arrival.jpg"),
        ])
    }

    async fn detail(
        &self,
        kind: MetadataItemKind,
        id: u64,
        language: &str,
    ) -> Result<MetadataCandidate, MetadataProviderError> {
        self.detail_calls
            .lock()
            .unwrap()
            .push((kind, id, language.to_owned()));
        let source = MetadataSource::new("Tmdb", Some("movie:329865"), 8_000).unwrap();
        Ok(MetadataCandidate::new(source)
            .with_title("Arrival")
            .with_original_title("Arrival")
            .with_year(2016)
            .with_overview("A linguist meets visitors.")
            .with_provider_id("tmdb", "329865")
            .with_primary_image("/arrival.jpg")
            .with_community_rating(7.6)
            .with_vote_count(18_000)
            .with_runtime_ticks(69_600_000_000)
            .with_release_status("Released")
            .with_official_rating("PG-13")
            .with_original_language("en")
            .with_genres(vec!["Drama".to_owned(), "Science Fiction".to_owned()])
            .with_studios(vec!["Paramount Pictures".to_owned()])
            .with_countries(vec![
                MetadataNamedValue::new("US", "United States").unwrap(),
            ])
            .with_languages(vec![MetadataNamedValue::new("en", "English").unwrap()])
            .with_people(vec![
                MetadataPerson::new("Amy Adams", Some("Louise Banks"), Some(0)).unwrap(),
            ])
            .with_details_loaded())
    }
}

#[tokio::test]
async fn tmdb_provider_selects_the_requested_year_and_maps_basic_metadata() {
    let transport = Arc::new(FakeTransport {
        calls: Mutex::new(Vec::new()),
        detail_calls: Mutex::new(Vec::new()),
        validation_result: Ok(()),
    });
    let provider = TmdbProvider::with_transport("zh-CN", transport.clone()).unwrap();
    let lookup = MetadataLookup::new(MetadataItemKind::Movie, "Arrival", Some(2016)).unwrap();

    let candidate = provider.resolve(&lookup).await.unwrap().unwrap();
    let resolution = tjxy_metadata::MetadataResolution::from_candidate(&lookup, candidate).unwrap();

    assert_eq!(resolution.title(), "Arrival");
    assert_eq!(resolution.production_year(), Some(2016));
    assert_eq!(resolution.overview(), Some("A linguist meets visitors."));
    assert_eq!(resolution.provider_ids().get("tmdb").unwrap(), "329865");
    assert!(resolution.details_loaded());
    assert_eq!(resolution.community_rating(), Some(7.6));
    assert_eq!(resolution.runtime_ticks(), Some(69_600_000_000));
    assert_eq!(resolution.genres().unwrap(), ["Drama", "Science Fiction"]);
    let poster = resolution.primary_image().unwrap();
    assert_eq!(poster.provider(), "Tmdb");
    assert_eq!(poster.reference(), "/arrival.jpg");
    assert_eq!(poster.url(), "https://image.tmdb.org/t/p/w500/arrival.jpg");
    assert_eq!(
        resolution.provenance("title").unwrap().reference(),
        Some("movie:329865")
    );
    assert_eq!(
        transport.calls.lock().unwrap().as_slice(),
        [(
            MetadataItemKind::Movie,
            "Arrival".to_owned(),
            Some(2016),
            "zh-CN".to_owned()
        )]
    );
    assert_eq!(
        transport.detail_calls.lock().unwrap().as_slice(),
        [(MetadataItemKind::Movie, 329_865, "zh-CN".to_owned())]
    );
}

#[tokio::test]
async fn title_search_is_not_used_for_season_or_episode_lookups() {
    let transport = Arc::new(FakeTransport {
        calls: Mutex::new(Vec::new()),
        detail_calls: Mutex::new(Vec::new()),
        validation_result: Ok(()),
    });
    let provider = TmdbProvider::with_transport("en-AU", transport.clone()).unwrap();
    let lookup = MetadataLookup::new(MetadataItemKind::Episode, "Pilot", None).unwrap();

    assert!(provider.resolve(&lookup).await.unwrap().is_none());
    assert!(transport.calls.lock().unwrap().is_empty());
    assert!(transport.detail_calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn connection_validation_forwards_the_transport_result_without_searching() {
    let accepted_transport = Arc::new(FakeTransport {
        calls: Mutex::new(Vec::new()),
        detail_calls: Mutex::new(Vec::new()),
        validation_result: Ok(()),
    });
    let accepted = TmdbProvider::with_transport("en-AU", accepted_transport.clone()).unwrap();

    accepted.validate_connection().await.unwrap();

    assert!(accepted_transport.calls.lock().unwrap().is_empty());

    let rejected_transport = Arc::new(FakeTransport {
        calls: Mutex::new(Vec::new()),
        detail_calls: Mutex::new(Vec::new()),
        validation_result: Err(MetadataProviderError::Rejected),
    });
    let rejected = TmdbProvider::with_transport("en-AU", rejected_transport.clone()).unwrap();

    assert_eq!(
        rejected.validate_connection().await,
        Err(MetadataProviderError::Rejected)
    );
    assert!(rejected_transport.calls.lock().unwrap().is_empty());
}
