use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use async_trait::async_trait;
use serde_json::json;
use tjxy_metadata::{
    MetadataItemKind, MetadataProviderError, TmdbCatalogClient, TmdbCatalogTransport,
};

type TransportCall = (String, Vec<(String, String)>);

struct FixtureTransport {
    responses: BTreeMap<String, Vec<u8>>,
    calls: Mutex<Vec<TransportCall>>,
}

#[async_trait]
impl TmdbCatalogTransport for FixtureTransport {
    async fn get(
        &self,
        path: &str,
        query: &[(String, String)],
    ) -> Result<Vec<u8>, MetadataProviderError> {
        self.calls
            .lock()
            .unwrap()
            .push((path.to_owned(), query.to_vec()));
        self.responses
            .get(path)
            .cloned()
            .ok_or(MetadataProviderError::Rejected)
    }
}

fn response(value: &serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(&value).unwrap()
}

fn minimal_movie_response() -> Vec<u8> {
    response(&json!({
        "id": 329_865,
        "title": "Arrival",
        "overview": "A linguist communicates with visitors.",
        "release_date": "2016-11-10",
        "runtime": 116,
        "status": "Released",
        "vote_average": 8.1,
        "vote_count": 19000,
        "poster_path": "/arrival-poster.jpg",
        "original_language": "en"
    }))
}

#[tokio::test]
async fn popular_pages_return_bounded_unique_ids_for_manifest_generation() {
    let transport = Arc::new(FixtureTransport {
        responses: BTreeMap::from([
            (
                "/movie/popular".to_owned(),
                response(&json!({
                    "page": 3,
                    "total_pages": 50_000,
                    "results": [
                        {"id": 329_865, "title": "Arrival", "adult": false},
                        {"id": 496_243, "title": "Parasite", "adult": false},
                        {"id": 329_865, "title": "Arrival", "adult": false}
                    ]
                })),
            ),
            (
                "/tv/popular".to_owned(),
                response(&json!({
                    "page": 2,
                    "total_pages": 250,
                    "results": [
                        {"id": 87108, "name": "Chernobyl", "adult": false},
                        {"id": 87739, "name": "The Queen's Gambit", "adult": false}
                    ]
                })),
            ),
        ]),
        calls: Mutex::new(Vec::new()),
    });
    let client = TmdbCatalogClient::with_transport("zh-CN", transport.clone()).unwrap();

    assert_eq!(
        client.popular_movie_ids(3).await.unwrap(),
        [329_865, 496_243]
    );
    assert_eq!(
        client.popular_series_ids(2).await.unwrap(),
        [87_108, 87_739]
    );
    assert_eq!(
        transport.calls.lock().unwrap().as_slice(),
        [
            (
                "/movie/popular".to_owned(),
                vec![
                    ("language".to_owned(), "zh-CN".to_owned()),
                    ("page".to_owned(), "3".to_owned())
                ]
            ),
            (
                "/tv/popular".to_owned(),
                vec![
                    ("language".to_owned(), "zh-CN".to_owned()),
                    ("page".to_owned(), "2".to_owned())
                ]
            )
        ]
    );
}

#[tokio::test]
async fn popular_pages_expose_lightweight_ranked_summaries_without_detail_requests() {
    let transport = Arc::new(FixtureTransport {
        responses: BTreeMap::from([(
            "/movie/popular".to_owned(),
            response(&json!({
                "page": 1,
                "total_pages": 10,
                "results": [{
                    "id": 329_865,
                    "title": "Arrival",
                    "overview": "A linguist communicates with visitors.",
                    "release_date": "2016-11-10",
                    "poster_path": "/arrival.jpg",
                    "vote_average": 8.1,
                    "popularity": 120.4
                }]
            })),
        )]),
        calls: Mutex::new(Vec::new()),
    });
    let client = TmdbCatalogClient::with_transport("zh-CN", transport.clone()).unwrap();

    let items = client.popular_movies(1).await.unwrap();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id(), 329_865);
    assert_eq!(items[0].name(), "Arrival");
    assert_eq!(items[0].year(), Some(2016));
    assert_eq!(items[0].rating(), Some(8.1));
    assert_eq!(
        items[0].poster_url(),
        Some("https://image.tmdb.org/t/p/w500/arrival.jpg")
    );
    assert_eq!(transport.calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn movie_details_map_rich_metadata_and_bounded_credits() {
    let mut cast = (0_u64..30)
        .map(|index| {
            json!({
                "id": 1000 + index,
                "name": format!("Actor {index}"),
                "character": format!("Character {index}"),
                "order": index,
                "profile_path": format!("/actor-{index}.jpg")
            })
        })
        .collect::<Vec<_>>();
    cast.reverse();
    let transport = Arc::new(FixtureTransport {
        responses: BTreeMap::from([(
            "/movie/329865".to_owned(),
            response(&json!({
                "id": 329_865,
                "title": "降临",
                "original_title": "Arrival",
                "overview": "一位语言学家尝试与访客沟通。",
                "tagline": "为什么他们会来到这里？",
                "release_date": "2016-11-10",
                "runtime": 116,
                "status": "Released",
                "vote_average": 8.1,
                "vote_count": 19000,
                "poster_path": "/arrival-poster.jpg",
                "backdrop_path": "/arrival-backdrop.jpg",
                "original_language": "en",
                "genres": [{"id": 878, "name": "科幻"}],
                "production_companies": [{"id": 1, "name": "FilmNation Entertainment"}],
                "production_countries": [{"iso_3166_1": "US", "name": "United States of America"}],
                "spoken_languages": [{"english_name": "English", "iso_639_1": "en", "name": "English"}],
                "credits": {
                    "cast": cast,
                    "crew": [
                        {"id": 10, "name": "Denis Villeneuve", "job": "Director", "department": "Directing", "profile_path": "/denis.jpg"},
                        {"id": 11, "name": "Eric Heisserer", "job": "Screenplay", "department": "Writing", "profile_path": null}
                    ]
                },
                "release_dates": {
                    "results": [{
                        "iso_3166_1": "US",
                        "release_dates": [{"certification": "PG-13", "type": 3}]
                    }]
                },
                "external_ids": {"imdb_id": "tt2543164", "wikidata_id": "Q20382729"},
                "images": {"posters": [], "backdrops": []}
            })),
        )]),
        calls: Mutex::new(Vec::new()),
    });
    let client = TmdbCatalogClient::with_transport("zh-CN", transport.clone()).unwrap();

    let movie = client.movie(329_865).await.unwrap();

    assert_eq!(movie.kind(), MetadataItemKind::Movie);
    assert_eq!(movie.provider_id(), 329_865);
    assert_eq!(movie.title(), "降临");
    assert_eq!(movie.original_title(), Some("Arrival"));
    assert_eq!(movie.community_rating(), Some(8.1));
    assert_eq!(movie.vote_count(), Some(19_000));
    assert_eq!(movie.runtime_ticks(), Some(69_600_000_000));
    assert_eq!(movie.premiere_date().unwrap().to_string(), "2016-11-10");
    assert_eq!(movie.official_rating(), Some("PG-13"));
    assert_eq!(movie.genres(), ["科幻"]);
    assert_eq!(movie.countries()[0].code(), "US");
    assert_eq!(movie.languages()[0].code(), "en");
    assert_eq!(movie.credits().len(), 26);
    assert_eq!(movie.credits()[0].person_name(), "Actor 0");
    assert_eq!(movie.credits()[23].person_name(), "Actor 23");
    assert_eq!(movie.credits()[24].credit_type(), "Director");
    assert_eq!(
        movie.provider_ids().get("imdb").map(String::as_str),
        Some("tt2543164")
    );
    assert_eq!(movie.images().len(), 2);
    assert_eq!(movie.images()[0].path(), "/arrival-poster.jpg");
    assert_eq!(
        transport.calls.lock().unwrap().as_slice(),
        [(
            "/movie/329865".to_owned(),
            vec![
                ("language".to_owned(), "zh-CN".to_owned()),
                (
                    "append_to_response".to_owned(),
                    "credits,release_dates,external_ids,images".to_owned()
                ),
                ("include_image_language".to_owned(), "zh,en,null".to_owned())
            ]
        )]
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keeps the complete Series and Season wire fixture together.
async fn series_details_fetch_every_season_and_map_ordered_episodes() {
    let transport = Arc::new(FixtureTransport {
        responses: BTreeMap::from([
            (
                "/tv/87108".to_owned(),
                response(&json!({
                    "id": 87108,
                    "name": "切尔诺贝利",
                    "original_name": "Chernobyl",
                    "overview": "灾难之后的真相。",
                    "tagline": "真相不会消失。",
                    "first_air_date": "2019-05-06",
                    "last_air_date": "2019-06-03",
                    "status": "Ended",
                    "vote_average": 8.7,
                    "vote_count": 7000,
                    "episode_run_time": [69],
                    "poster_path": "/chernobyl.jpg",
                    "backdrop_path": "/chernobyl-bg.jpg",
                    "original_language": "en",
                    "genres": [{"id": 18, "name": "剧情"}],
                    "production_companies": null,
                    "production_countries": [{"iso_3166_1": "US", "name": "United States of America"}],
                    "spoken_languages": [{"english_name": "English", "iso_639_1": "en", "name": "English"}],
                    "seasons": [
                        {"id": 120_000, "season_number": 1, "name": "第 1 季", "episode_count": 2}
                    ],
                    "aggregate_credits": {
                        "cast": [{
                            "id": 20,
                            "name": "Jared Harris",
                            "order": 0,
                            "roles": [{"character": "Valery Legasov", "episode_count": 5}],
                            "profile_path": "/jared.jpg"
                        }],
                        "crew": [{
                            "id": 21,
                            "name": "Craig Mazin",
                            "department": "Writing",
                            "jobs": [{"job": "Writer", "episode_count": 5}],
                            "profile_path": "/craig.jpg"
                        }]
                    },
                    "content_ratings": {"results": [{"iso_3166_1": "US", "rating": "TV-MA"}]},
                    "external_ids": null,
                    "images": {"posters": [], "backdrops": []}
                })),
            ),
            (
                "/tv/87108/season/1".to_owned(),
                response(&json!({
                    "id": 120_000,
                    "name": "第 1 季",
                    "overview": "五集限定剧。",
                    "air_date": "2019-05-06",
                    "season_number": 1,
                    "poster_path": "/chernobyl-s1.jpg",
                    "episodes": [
                        {
                            "id": 170_002,
                            "name": "Please Remain Calm",
                            "overview": "第二集",
                            "air_date": "2019-05-13",
                            "episode_number": 2,
                            "season_number": 1,
                            "runtime": 65,
                            "vote_average": 8.5,
                            "vote_count": 500,
                            "still_path": "/e2.jpg",
                            "guest_stars": null,
                            "crew": null
                        },
                        {
                            "id": 170_001,
                            "name": "1:23:45",
                            "overview": "第一集\n灾难开始\t调查展开",
                            "air_date": "2019-05-06",
                            "episode_number": 1,
                            "season_number": 1,
                            "runtime": 59,
                            "vote_average": 8.7,
                            "vote_count": 600,
                            "still_path": "/e1.jpg",
                            "guest_stars": [{
                                "id": 22,
                                "name": "Guest Actor",
                                "character": "Worker",
                                "order": 0,
                                "profile_path": null
                            }],
                            "crew": [{
                                "id": 23,
                                "name": "Johan Renck",
                                "job": "Director",
                                "department": "Directing",
                                "profile_path": "/johan.jpg"
                            }]
                        }
                    ],
                    "credits": null,
                    "images": {"posters": []}
                })),
            ),
        ]),
        calls: Mutex::new(Vec::new()),
    });
    let client = TmdbCatalogClient::with_transport("zh-CN", transport.clone()).unwrap();

    let series = client.series(87_108).await.unwrap();

    assert_eq!(series.item().kind(), MetadataItemKind::Series);
    assert_eq!(series.item().official_rating(), Some("TV-MA"));
    assert_eq!(series.seasons().len(), 1);
    assert_eq!(series.seasons()[0].item().kind(), MetadataItemKind::Season);
    assert_eq!(series.seasons()[0].item().index_number(), Some(1));
    assert_eq!(series.seasons()[0].episodes().len(), 2);
    let sampled = series.clone().with_structure_limits(1, 1);
    assert_eq!(sampled.seasons().len(), 1);
    assert_eq!(sampled.seasons()[0].episodes().len(), 1);
    assert_eq!(
        series.seasons()[0]
            .episodes()
            .iter()
            .map(|episode| episode.item().index_number().unwrap())
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(
        series.seasons()[0].episodes()[0].item().runtime_ticks(),
        Some(35_400_000_000)
    );
    assert_eq!(
        series.seasons()[0].episodes()[0].item().overview(),
        Some("第一集 灾难开始 调查展开")
    );
    assert_eq!(
        series.seasons()[0].episodes()[0].item().credits()[0].credit_type(),
        "Actor"
    );
    assert_eq!(
        series.seasons()[0].episodes()[0].item().credits()[1].credit_type(),
        "Director"
    );
    assert_eq!(
        transport
            .calls
            .lock()
            .unwrap()
            .iter()
            .map(|call| call.0.as_str())
            .collect::<Vec<_>>(),
        ["/tv/87108", "/tv/87108/season/1"]
    );
}

struct SequenceTransport {
    results: Mutex<VecDeque<Result<Vec<u8>, MetadataProviderError>>>,
    calls: AtomicUsize,
}

#[async_trait]
impl TmdbCatalogTransport for SequenceTransport {
    async fn get(
        &self,
        _path: &str,
        _query: &[(String, String)],
    ) -> Result<Vec<u8>, MetadataProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.results.lock().unwrap().pop_front().unwrap()
    }
}

#[tokio::test]
async fn temporary_failures_retry_three_times_but_rejections_do_not_retry() {
    let recovered = Arc::new(SequenceTransport {
        results: Mutex::new(VecDeque::from([
            Err(MetadataProviderError::TemporarilyUnavailable),
            Err(MetadataProviderError::TemporarilyUnavailable),
            Ok(minimal_movie_response()),
        ])),
        calls: AtomicUsize::new(0),
    });
    let client = TmdbCatalogClient::with_transport("en-US", recovered.clone()).unwrap();

    assert_eq!(client.movie(329_865).await.unwrap().title(), "Arrival");
    assert_eq!(recovered.calls.load(Ordering::SeqCst), 3);

    let rejected = Arc::new(SequenceTransport {
        results: Mutex::new(VecDeque::from([Err(MetadataProviderError::Rejected)])),
        calls: AtomicUsize::new(0),
    });
    let client = TmdbCatalogClient::with_transport("en-US", rejected.clone()).unwrap();

    assert_eq!(
        client.movie(329_865).await,
        Err(MetadataProviderError::Rejected)
    );
    assert_eq!(rejected.calls.load(Ordering::SeqCst), 1);
}

struct LanguageTransport {
    responses: BTreeMap<(String, String), Vec<u8>>,
    calls: Mutex<Vec<(String, String)>>,
}

#[async_trait]
impl TmdbCatalogTransport for LanguageTransport {
    async fn get(
        &self,
        path: &str,
        query: &[(String, String)],
    ) -> Result<Vec<u8>, MetadataProviderError> {
        let language = query
            .iter()
            .find(|(name, _)| name == "language")
            .map(|(_, value)| value.clone())
            .unwrap();
        self.calls
            .lock()
            .unwrap()
            .push((path.to_owned(), language.clone()));
        self.responses
            .get(&(path.to_owned(), language))
            .cloned()
            .ok_or(MetadataProviderError::Rejected)
    }
}

#[tokio::test]
async fn empty_localized_overview_falls_back_without_replacing_localized_title() {
    let localized = json!({
        "id": 329_865,
        "title": "降临",
        "overview": "",
        "release_date": "2016-11-10",
        "runtime": 116,
        "poster_path": "/arrival.jpg",
        "original_language": "en"
    });
    let english = json!({
        "id": 329_865,
        "title": "Arrival",
        "overview": "A linguist communicates with visitors.",
        "release_date": "2016-11-10",
        "runtime": 116,
        "poster_path": "/arrival.jpg",
        "original_language": "en"
    });
    let transport = Arc::new(LanguageTransport {
        responses: BTreeMap::from([
            (
                ("/movie/329865".to_owned(), "zh-CN".to_owned()),
                response(&localized),
            ),
            (
                ("/movie/329865".to_owned(), "en-US".to_owned()),
                response(&english),
            ),
        ]),
        calls: Mutex::new(Vec::new()),
    });
    let client = TmdbCatalogClient::with_transport("zh-CN", transport.clone()).unwrap();

    let movie = client.movie(329_865).await.unwrap();

    assert_eq!(movie.title(), "降临");
    assert_eq!(
        movie.overview(),
        Some("A linguist communicates with visitors.")
    );
    assert_eq!(
        transport.calls.lock().unwrap().as_slice(),
        [
            ("/movie/329865".to_owned(), "zh-CN".to_owned()),
            ("/movie/329865".to_owned(), "en-US".to_owned())
        ]
    );
}
