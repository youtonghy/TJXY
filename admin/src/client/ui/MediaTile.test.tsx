import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import type { MediaItem } from '../api/catalogApi';
import { MediaTile } from './MediaTile';

vi.mock('./MediaImage', () => ({
  MediaImage: ({ alt }: { alt: string }) => <div aria-label={alt} role="img" />,
}));

it('shows a progress ring and favorite heart for a partially watched favorite', () => {
  renderTile({
    UserData: {
      IsFavorite: true,
      PlaybackPositionTicks: 300,
    },
    RunTimeTicks: 1_000,
  });

  expect(screen.getByRole('progressbar', { name: '30% watched' })).toHaveAttribute('aria-valuenow', '30');
  expect(screen.getByLabelText('Favorite')).toBeVisible();
  expect(screen.queryByLabelText('Watched')).not.toBeInTheDocument();
});

it('shows a completed check instead of progress after the title is watched', () => {
  renderTile({
    UserData: {
      IsFavorite: true,
      Played: true,
      PlaybackPositionTicks: 300,
    },
    RunTimeTicks: 1_000,
  });

  expect(screen.getByLabelText('Watched')).toBeVisible();
  expect(screen.getByLabelText('Favorite')).toBeVisible();
  expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
});

it('does not show playback status for an untouched title or invalid runtime', () => {
  renderTile({
    UserData: { PlaybackPositionTicks: 300 },
    RunTimeTicks: 0,
  });

  expect(screen.queryByLabelText('Watched')).not.toBeInTheDocument();
  expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
  expect(screen.queryByLabelText('Favorite')).not.toBeInTheDocument();
});

it('supports a context-specific destination without changing the default item link', () => {
  const { rerender } = renderTile({}, '/app/play/movie-1');

  expect(screen.getByRole('link', { name: /Example Movie/ })).toHaveAttribute('href', '/app/play/movie-1');

  rerender(
    <MemoryRouter>
      <MediaTile item={{ Id: 'movie-1', Name: 'Example Movie', Type: 'Movie' }} />
    </MemoryRouter>,
  );
  expect(screen.getByRole('link', { name: /Example Movie/ })).toHaveAttribute('href', '/app/items/movie-1');
});

it('uses square artwork for audio while preserving portrait movie posters', () => {
  const { rerender } = renderTile({ Type: 'Audio' });

  expect(screen.getByRole('img', { name: 'Poster for Example Movie' }).parentElement)
    .toHaveClass('aspect-square');

  rerender(
    <MemoryRouter>
      <MediaTile item={{ Id: 'movie-1', Name: 'Example Movie', Type: 'Movie' }} />
    </MemoryRouter>,
  );
  expect(screen.getByRole('img', { name: 'Poster for Example Movie' }).parentElement)
    .toHaveClass('aspect-[2/3]');
});

function renderTile(overrides: Partial<MediaItem>, to?: string) {
  return render(
    <MemoryRouter>
      <MediaTile
        item={{
          Id: 'movie-1',
          Name: 'Example Movie',
          Type: 'Movie',
          ...overrides,
        }}
        to={to}
      />
    </MemoryRouter>,
  );
}
