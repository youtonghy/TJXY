use tjxy_metadata::{MetadataItemKind, MetadataState, NfoDocument};

#[test]
fn movie_nfo_parses_basic_fields_associations_and_safe_entities() {
    let document = NfoDocument::parse(
        br#"<?xml version="1.0" encoding="UTF-8"?>
            <movie>
              <title>Blade Runner &amp; More &#x2014; Final</title>
              <originaltitle>Blade Runner</originaltitle>
              <year>1982</year>
              <plot>A replicant hunter returns.</plot>
              <uniqueid type="tmdb" default="true">78</uniqueid>
              <imdbid>tt0083658</imdbid>
              <genre>Science Fiction</genre>
              <genre>Drama</genre>
              <studio>Warner Bros.</studio>
              <actor><name>Harrison Ford</name><role>Rick Deckard</role><order>1</order></actor>
            </movie>"#,
        "movie.nfo",
    )
    .unwrap();

    assert_eq!(document.kind(), MetadataItemKind::Movie);
    assert_eq!(document.title(), Some("Blade Runner & More \u{2014} Final"));
    assert_eq!(document.original_title(), Some("Blade Runner"));
    assert_eq!(document.production_year(), Some(1982));
    assert_eq!(document.overview(), Some("A replicant hunter returns."));
    assert_eq!(document.provider_id("tmdb"), Some("78"));
    assert_eq!(document.provider_id("imdb"), Some("tt0083658"));
    assert_eq!(document.genres(), ["Science Fiction", "Drama"]);
    assert_eq!(document.studios(), ["Warner Bros."]);
    assert_eq!(document.people().len(), 1);
    assert_eq!(document.people()[0].name(), "Harrison Ford");
    assert_eq!(document.people()[0].role(), Some("Rick Deckard"));
    assert_eq!(document.people()[0].order(), Some(1));
    assert_eq!(document.state(), MetadataState::Ready);
    assert_eq!(document.source().provider(), "Nfo");
    assert_eq!(document.source().reference(), Some("movie.nfo"));
}

#[test]
fn nfo_rejects_doctype_unknown_entities_and_oversized_input() {
    for xml in [
        br#"<!DOCTYPE movie [<!ENTITY xxe SYSTEM "file:///etc/passwd">]><movie><title>&xxe;</title></movie>"#.as_slice(),
        br"<movie><title>&custom;</title></movie>".as_slice(),
    ] {
        assert!(NfoDocument::parse(xml, "unsafe.nfo").is_err());
    }

    let oversized = vec![b'x'; NfoDocument::MAX_BYTES + 1];
    assert!(NfoDocument::parse(&oversized, "large.nfo").is_err());
}

#[test]
fn emby_series_nfo_accepts_bom_multiline_cdata_and_rich_fields() {
    let xml = [
        b"\xEF\xBB\xBF".as_slice(),
        br#"<?xml version="1.0" encoding="utf-8"?>
            <tvshow>
              <plot><![CDATA[First line.
Second line.]]></plot>
              <outline>Fallback outline.</outline>
              <title>Star Detective Precure!</title>
              <originaltitle>Detective Precure!</originaltitle>
              <rating>8.2</rating>
              <votes>1,234</votes>
              <year>2026</year>
              <mpaa>BR-10</mpaa>
              <imdb_id>tt39047437</imdb_id>
              <premiered>2026-01-31</premiered>
              <releasedate>2026-02-01</releasedate>
              <enddate>2026-12-31</enddate>
              <runtime>24</runtime>
              <status>Continuing</status>
              <language>ja</language>
              <uniqueid type="tmdb">306721</uniqueid>
            </tvshow>"#,
    ]
    .concat();

    let document = NfoDocument::parse(&xml, "tvshow.nfo").unwrap();

    assert_eq!(document.kind(), MetadataItemKind::Series);
    assert_eq!(document.overview(), Some("First line.\nSecond line."));
    assert_eq!(document.community_rating(), Some(8.2));
    assert_eq!(document.vote_count(), Some(1_234));
    assert_eq!(document.runtime_ticks(), Some(14_400_000_000));
    assert_eq!(
        document.premiere_date().map(|date| date.to_string()),
        Some("2026-01-31".to_owned())
    );
    assert_eq!(
        document.end_date().map(|date| date.to_string()),
        Some("2026-12-31".to_owned())
    );
    assert_eq!(document.release_status(), Some("Continuing"));
    assert_eq!(document.official_rating(), Some("BR-10"));
    assert_eq!(document.original_language(), Some("ja"));
    assert_eq!(document.provider_id("imdb"), Some("tt39047437"));
    assert_eq!(document.provider_id("tmdb"), Some("306721"));
    assert_eq!(document.state(), MetadataState::Ready);
}

#[test]
fn nfo_rejects_non_xml_control_characters_without_calling_them_oversized() {
    let error = NfoDocument::parse(
        b"<movie><title>Unsafe\x01Title</title></movie>",
        "unsafe-control.nfo",
    )
    .unwrap_err();

    assert!(matches!(error, tjxy_metadata::MetadataError::InvalidText));
}
