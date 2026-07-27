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
