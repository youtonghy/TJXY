import { getItems, getLatest, searchHints } from './catalogApi';

const client = vi.hoisted(() => ({ clientRequest: vi.fn() }));
vi.mock('./clientApi', () => client);

beforeEach(() => {
  client.clientRequest.mockResolvedValue({ Items: [], SearchHints: [] });
});

it('uses the server camelCase query contract for catalog reads', async () => {
  await getItems({ parentId: 'library-1', startIndex: 24, limit: 12, includeItemTypes: 'Movie' });
  await getLatest(18);
  await searchHints('arrival');

  expect(client.clientRequest).toHaveBeenNthCalledWith(
    1,
    '/Items?limit=12&startIndex=24&parentId=library-1&includeItemTypes=Movie',
  );
  expect(client.clientRequest).toHaveBeenNthCalledWith(2, '/Items/Latest?limit=18');
  expect(client.clientRequest).toHaveBeenNthCalledWith(3, '/Search/Hints?searchTerm=arrival&limit=24');
});
