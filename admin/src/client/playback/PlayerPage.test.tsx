import { render, screen } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { PlayerPage } from './PlayerPage';

const catalog = vi.hoisted(() => ({ getItem: vi.fn() }));
const playback = vi.hoisted(() => ({
  getPlaybackInfo: vi.fn(),
  issuePlaybackTicket: vi.fn(),
  revokePlaybackTicket: vi.fn(),
}));

vi.mock('../api/catalogApi', () => catalog);
vi.mock('../api/playbackApi', () => playback);

it('shows the metadata-only state without requesting playback for a demo title', async () => {
  catalog.getItem.mockResolvedValue({
    Id: 'movie-1',
    Name: 'Arrival',
    Type: 'Movie',
    HasMediaSources: false,
  });

  render(
    <MemoryRouter initialEntries={['/app/play/movie-1']}>
      <Routes>
        <Route element={<PlayerPage />} path="/app/play/:id" />
      </Routes>
    </MemoryRouter>,
  );

  expect(await screen.findByText('Demo title, no video file attached')).toBeInTheDocument();
  expect(screen.getByRole('link', { name: 'Back to details' })).toHaveAttribute('href', '/app/items/movie-1');
  expect(screen.queryByRole('video')).not.toBeInTheDocument();
  expect(document.querySelector('video')).toBeNull();
  expect(playback.getPlaybackInfo).not.toHaveBeenCalled();
  expect(playback.issuePlaybackTicket).not.toHaveBeenCalled();
});
