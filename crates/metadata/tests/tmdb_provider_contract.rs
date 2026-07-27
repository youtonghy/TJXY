use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tjxy_metadata::{
    MetadataItemKind, MetadataLookup, MetadataProvider, MetadataProviderError, TmdbProvider,
    TmdbSearchItem, TmdbTransport,
};

type SearchCall = (MetadataItemKind, String, Option<i32>, String);

struct FakeTransport {
    calls: Mutex<Vec<SearchCall>>,
}

#[async_trait]
impl TmdbTransport for FakeTransport {
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
}

#[tokio::test]
async fn tmdb_provider_selects_the_requested_year_and_maps_basic_metadata() {
    let transport = Arc::new(FakeTransport {
        calls: Mutex::new(Vec::new()),
    });
    let provider = TmdbProvider::with_transport("zh-CN", transport.clone()).unwrap();
    let lookup = MetadataLookup::new(MetadataItemKind::Movie, "Arrival", Some(2016)).unwrap();

    let candidate = provider.resolve(&lookup).await.unwrap().unwrap();
    let resolution = tjxy_metadata::MetadataResolution::from_candidate(&lookup, candidate).unwrap();

    assert_eq!(resolution.title(), "Arrival");
    assert_eq!(resolution.production_year(), Some(2016));
    assert_eq!(resolution.overview(), Some("A linguist meets visitors."));
    assert_eq!(resolution.provider_ids().get("tmdb").unwrap(), "329865");
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
}

#[tokio::test]
async fn title_search_is_not_used_for_season_or_episode_lookups() {
    let transport = Arc::new(FakeTransport {
        calls: Mutex::new(Vec::new()),
    });
    let provider = TmdbProvider::with_transport("en-AU", transport.clone()).unwrap();
    let lookup = MetadataLookup::new(MetadataItemKind::Episode, "Pilot", None).unwrap();

    assert!(provider.resolve(&lookup).await.unwrap().is_none());
    assert!(transport.calls.lock().unwrap().is_empty());
}
