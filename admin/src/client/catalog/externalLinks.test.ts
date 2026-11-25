import { buildExternalLinks } from './externalLinks';

const series = { Id: 'series-1', Name: 'Breaking Bad', Type: 'Series', ProviderIds: { tmdb: '1396', imdb: 'tt0903747', tvdb: '81189' } };
const season = { Id: 'season-1', Name: 'Season 1', Type: 'Season', IndexNumber: 1, ProviderIds: { tvdb: '30272' } };

it('links a movie to TMDB, IMDb, and TVDB pages', () => {
  expect(buildExternalLinks({ Id: 'm', Name: 'Movie', Type: 'Movie', ProviderIds: { Tmdb: '603', Imdb: 'tt0133093', Tvdb: '169' } }))
    .toEqual([
      { provider: 'tmdb', label: 'TMDB', url: 'https://www.themoviedb.org/movie/603' },
      { provider: 'imdb', label: 'IMDb', url: 'https://www.imdb.com/title/tt0133093/' },
      { provider: 'tvdb', label: 'TVDB', url: 'https://thetvdb.com/dereferrer/movie/169' },
    ]);
});

it('links a series to the TMDB tv page', () => {
  expect(buildExternalLinks(series).map((link) => link.url)).toEqual([
    'https://www.themoviedb.org/tv/1396',
    'https://www.imdb.com/title/tt0903747/',
    'https://thetvdb.com/dereferrer/series/81189',
  ]);
});

it('derives season and episode TMDB pages from the parent series', () => {
  expect(buildExternalLinks(season, [series]).map((link) => link.url)).toEqual([
    'https://www.themoviedb.org/tv/1396/season/1',
    'https://thetvdb.com/dereferrer/season/30272',
  ]);
  const episode = { Id: 'e', Name: 'Pilot', Type: 'Episode', IndexNumber: 1, ProviderIds: { tmdb: '62085', imdb: 'tt0959621', tvdb: '349232' } };
  expect(buildExternalLinks(episode, [series, season]).map((link) => link.url)).toEqual([
    'https://www.themoviedb.org/tv/1396/season/1/episode/1',
    'https://www.imdb.com/title/tt0959621/',
    'https://thetvdb.com/dereferrer/episode/349232',
  ]);
});

it('keeps season zero specials and omits TMDB when numbering is unknown', () => {
  const specials = { ...season, IndexNumber: 0 };
  expect(buildExternalLinks({ Id: 'e', Name: 'Special', Type: 'Episode', IndexNumber: 3 }, [series, specials])[0]?.url)
    .toBe('https://www.themoviedb.org/tv/1396/season/0/episode/3');
  expect(buildExternalLinks({ Id: 'e', Name: 'Loose', Type: 'Episode', IndexNumber: 3 }, [series])).toEqual([]);
  expect(buildExternalLinks({ Id: 'e', Name: 'Unnumbered', Type: 'Episode' }, [series, season])).toEqual([]);
});

it('ignores malformed IDs and unsupported item types', () => {
  expect(buildExternalLinks({ Id: 'm', Name: 'Bad', Type: 'Movie', ProviderIds: { tmdb: '12a', imdb: '0133093', tvdb: '../1' } })).toEqual([]);
  expect(buildExternalLinks({ Id: 'a', Name: 'Track', Type: 'Audio', ProviderIds: { tmdb: '1', imdb: 'tt0133093', tvdb: '1' } })).toEqual([]);
});
