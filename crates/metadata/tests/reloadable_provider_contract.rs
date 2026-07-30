use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tjxy_metadata::{
    MetadataCandidate, MetadataItemKind, MetadataLookup, MetadataProvider, MetadataProviderError,
    MetadataResolution, MetadataSource, ReloadableMetadataProvider,
};
use tokio::sync::oneshot;

struct FixtureProvider {
    title: &'static str,
}

#[async_trait]
impl MetadataProvider for FixtureProvider {
    fn name(&self) -> &'static str {
        "Fixture"
    }

    async fn resolve(
        &self,
        _lookup: &MetadataLookup,
    ) -> Result<Option<MetadataCandidate>, MetadataProviderError> {
        Ok(Some(fixture_candidate(self.title)))
    }
}

struct BlockingFixtureProvider {
    title: &'static str,
    started: Mutex<Option<oneshot::Sender<()>>>,
    release: Mutex<Option<oneshot::Receiver<()>>>,
}

#[async_trait]
impl MetadataProvider for BlockingFixtureProvider {
    fn name(&self) -> &'static str {
        "Fixture"
    }

    async fn resolve(
        &self,
        _lookup: &MetadataLookup,
    ) -> Result<Option<MetadataCandidate>, MetadataProviderError> {
        self.started
            .lock()
            .unwrap()
            .take()
            .unwrap()
            .send(())
            .unwrap();
        let release = self.release.lock().unwrap().take().unwrap();
        release.await.unwrap();
        Ok(Some(fixture_candidate(self.title)))
    }
}

fn fixture_candidate(title: &str) -> MetadataCandidate {
    let source = MetadataSource::new("Fixture", Option::<String>::None, 8_000).unwrap();
    MetadataCandidate::new(source).with_title(title)
}

fn lookup() -> MetadataLookup {
    MetadataLookup::new(MetadataItemKind::Movie, "Fallback", None).unwrap()
}

fn candidate_title(candidate: MetadataCandidate) -> String {
    MetadataResolution::from_candidate(&lookup(), candidate)
        .unwrap()
        .title()
        .to_owned()
}

#[tokio::test]
async fn empty_reloadable_provider_returns_none() {
    let provider = ReloadableMetadataProvider::new("Tmdb");

    assert_eq!(provider.name(), "Tmdb");
    assert!(provider.resolve(&lookup()).await.unwrap().is_none());
}

#[tokio::test]
async fn replacement_is_used_by_the_next_resolution() {
    let provider = ReloadableMetadataProvider::new("Tmdb");
    provider.replace(Some(Arc::new(FixtureProvider {
        title: "Replacement Title",
    })));

    let candidate = provider.resolve(&lookup()).await.unwrap().unwrap();

    assert_eq!(candidate_title(candidate), "Replacement Title");
}

#[tokio::test]
async fn disabling_the_provider_returns_none() {
    let provider = ReloadableMetadataProvider::new("Tmdb");
    provider.replace(Some(Arc::new(FixtureProvider {
        title: "Enabled Title",
    })));
    provider.replace(None);

    assert!(provider.resolve(&lookup()).await.unwrap().is_none());
}

#[tokio::test]
async fn in_flight_resolution_keeps_its_provider_snapshot() {
    let (started, started_receiver) = oneshot::channel();
    let (release, release_receiver) = oneshot::channel();
    let provider = Arc::new(ReloadableMetadataProvider::new("Tmdb"));
    provider.replace(Some(Arc::new(BlockingFixtureProvider {
        title: "Old Title",
        started: Mutex::new(Some(started)),
        release: Mutex::new(Some(release_receiver)),
    })));

    let in_flight = {
        let provider = provider.clone();
        tokio::spawn(async move { provider.resolve(&lookup()).await.unwrap().unwrap() })
    };
    started_receiver.await.unwrap();
    provider.replace(Some(Arc::new(FixtureProvider {
        title: "Replacement Title",
    })));
    release.send(()).unwrap();

    let old_candidate = in_flight.await.unwrap();
    let replacement_candidate = provider.resolve(&lookup()).await.unwrap().unwrap();

    assert_eq!(candidate_title(old_candidate), "Old Title");
    assert_eq!(candidate_title(replacement_candidate), "Replacement Title");
}
