import { act, render, screen, waitFor } from '@testing-library/react';

import { ClientAuthProvider, useClientAuth } from './ClientAuthContext';

const api = vi.hoisted(() => ({ clientRequest: vi.fn() }));
const cache = vi.hoisted(() => ({ clearPlaybackInfoCache: vi.fn() }));

vi.mock('../api/clientApi', () => ({
  CLIENT_AUTH_INVALIDATED_EVENT: 'tjxy-client-auth-invalidated',
  ClientApiError: class ClientApiError extends Error {},
  clientRequest: api.clientRequest,
}));
vi.mock('../playback/playbackInfoCache', () => cache);

function CurrentUser() {
  const { user } = useClientAuth();
  return <p>{user ? `signed in as ${user.Name}` : 'signed out'}</p>;
}

it('drops cached playback when the browser session switches accounts', async () => {
  api.clientRequest
    .mockResolvedValueOnce({ Id: 'user-a', Name: 'Alice' })
    .mockResolvedValueOnce({ Id: 'user-a', Name: 'Alice' })
    .mockResolvedValueOnce({ Id: 'user-b', Name: 'Bob' });
  render(<ClientAuthProvider><CurrentUser /></ClientAuthProvider>);
  expect(await screen.findByText('signed in as Alice')).toBeInTheDocument();

  act(() => { window.dispatchEvent(new Event('focus')); });
  await waitFor(() => { expect(api.clientRequest).toHaveBeenCalledTimes(2); });
  expect(cache.clearPlaybackInfoCache).not.toHaveBeenCalled();

  act(() => { window.dispatchEvent(new Event('focus')); });
  expect(await screen.findByText('signed in as Bob')).toBeInTheDocument();
  expect(cache.clearPlaybackInfoCache).toHaveBeenCalledTimes(1);
});
