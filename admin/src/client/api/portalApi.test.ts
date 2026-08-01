import {
  changePassword,
  getProfile,
  getServerRanking,
  getTmdbRanking,
  getUserInsights,
  updateProfile,
} from './portalApi';

const client = vi.hoisted(() => ({ clientRequest: vi.fn() }));
vi.mock('./clientApi', () => client);

beforeEach(() => { client.clientRequest.mockResolvedValue({ Items: [] }); });

it('uses self-scoped profile and insight routes', async () => {
  await getProfile();
  await updateProfile({ Bio: 'Bio', CurrentPassword: 'old', Username: 'new-name' });
  await changePassword({ CurrentPassword: 'old', NewPassword: 'new-password' });
  await getUserInsights('30d');
  await getTmdbRanking('Movie');
  await getServerRanking();

  expect(client.clientRequest).toHaveBeenNthCalledWith(1, '/Users/Me/Profile');
  expect(client.clientRequest).toHaveBeenNthCalledWith(2, '/Users/Me/Profile', expect.objectContaining({ method: 'PATCH' }));
  expect(client.clientRequest).toHaveBeenNthCalledWith(3, '/Users/Me/Password', expect.objectContaining({ method: 'POST' }));
  expect(client.clientRequest).toHaveBeenNthCalledWith(4, '/Users/Me/Insights?range=30d');
  expect(client.clientRequest).toHaveBeenNthCalledWith(5, '/Discover/Tmdb/Popular?mediaType=Movie');
  expect(client.clientRequest).toHaveBeenNthCalledWith(6, '/Discover/Server/Top?period=yesterday&limit=20');
});
