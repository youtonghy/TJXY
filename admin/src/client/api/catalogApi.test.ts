import { getItems, getLatest, getLibraryFilterFacets, getPopular, getResumeItems, getSimilarItems, latestTypesForLibrary, searchHints } from './catalogApi';

const client = vi.hoisted(() => ({ clientRequest: vi.fn() }));
vi.mock('./clientApi', () => client);

beforeEach(() => {
  client.clientRequest.mockReset();
  client.clientRequest.mockResolvedValue({ Items: [], SearchHints: [] });
});

it('uses the server camelCase query contract for catalog reads', async () => {
  await getItems({
    genre: 'Drama',
    includeItemTypes: 'Movie',
    limit: 12,
    parentId: 'library-1',
    productionYear: 2016,
    recursive: true,
    sortBy: 'ProductionYear',
    sortOrder: 'Descending',
    startIndex: 24,
  });
  await getLibraryFilterFacets('library / films');
  await getLatest({ limit: 12, parentId: 'library / films', includeItemTypes: 'Movie' });
  await getResumeItems(12);
  await searchHints('arrival');

  expect(client.clientRequest).toHaveBeenNthCalledWith(
    1,
    '/Items?limit=12&startIndex=24&parentId=library-1&includeItemTypes=Movie&genre=Drama&productionYear=2016&recursive=true&sortBy=ProductionYear&sortOrder=Descending',
  );
  expect(client.clientRequest).toHaveBeenNthCalledWith(
    2,
    '/Items/Filters?parentId=library+%2F+films',
  );
  expect(client.clientRequest).toHaveBeenNthCalledWith(
    3,
    '/Items/Latest?limit=12&parentId=library+%2F+films&includeItemTypes=Movie',
  );
  expect(client.clientRequest).toHaveBeenNthCalledWith(
    4,
    '/UserItems/Resume?mediaTypes=Video&limit=12&enableUserData=true',
  );
  expect(client.clientRequest).toHaveBeenNthCalledWith(5, '/Search/Hints?searchTerm=arrival&limit=24');
});

it('hydrates popular summaries with full catalog records for poster metadata', async () => {
  client.clientRequest
    .mockResolvedValueOnce({ Items: [{ Id: 'movie-1', Name: 'Arrival' }] })
    .mockResolvedValueOnce({ Id: 'movie-1', ImageTags: { Primary: 'poster-tag' }, Name: 'Arrival' });

  await expect(getPopular(12)).resolves.toEqual([
    { Id: 'movie-1', ImageTags: { Primary: 'poster-tag' }, Name: 'Arrival' },
  ]);
  expect(client.clientRequest).toHaveBeenNthCalledWith(1, '/Discover/Popular?limit=12');
  expect(client.clientRequest).toHaveBeenNthCalledWith(2, '/Items/movie-1');
});

it('requests audio items for a music library', () => {
  expect(latestTypesForLibrary({ Id: 'music', Name: 'Music', CollectionType: 'music' })).toBe('Audio');
});

it('requests a bounded standard page of similar items', async () => {
  client.clientRequest.mockResolvedValueOnce({
    Items: [{ Id: 'movie-2', Name: 'Similar Movie', Type: 'Movie' }],
    StartIndex: 0,
    TotalRecordCount: 1,
  });

  await expect(getSimilarItems('movie / 1', 4)).resolves.toEqual([
    { Id: 'movie-2', Name: 'Similar Movie', Type: 'Movie' },
  ]);
  expect(client.clientRequest).toHaveBeenCalledWith('/Items/movie%20%2F%201/Similar?limit=4');
});

it('rejects a malformed similar-item response instead of treating it as empty', async () => {
  client.clientRequest.mockResolvedValueOnce({ StartIndex: 0, TotalRecordCount: 0 });

  await expect(getSimilarItems('movie-1')).rejects.toThrow('invalid similar items response');
});
