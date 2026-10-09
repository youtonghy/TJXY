import { render, screen, waitFor } from '@testing-library/react';
import { MediaImage } from './MediaImage';

const api = vi.hoisted(() => ({
  clientBlob: vi.fn(),
  isRetryableClientError: vi.fn((error: unknown) => error instanceof Error && error.message === 'transient'),
}));
vi.mock('../api/clientApi', () => api);

beforeEach(() => {
  vi.clearAllMocks();
  api.clientBlob.mockResolvedValue(new Blob(['poster']));
  vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:poster');
  vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => undefined);
});

it('loads an original-location poster with library context even without an imported image tag', async () => {
  render(
    <MediaImage
      alt="Poster for Arrival"
      itemId="movie-1"
      libraryId="library-1"
    />,
  );

  await waitFor(() => {
    expect(api.clientBlob).toHaveBeenCalledWith(
      '/Items/movie-1/Images/Primary?libraryId=library-1',
      expect.any(AbortSignal),
    );
  });
  expect(await screen.findByRole('img', { name: 'Poster for Arrival' }))
    .toHaveAttribute('src', 'blob:poster');
});

it('retries a transient poster failure but gives up on permanent ones', async () => {
  vi.useFakeTimers();
  try {
    api.clientBlob.mockRejectedValueOnce(new Error('transient')).mockResolvedValue(new Blob(['poster']));
    render(<MediaImage alt="Poster" itemId="movie-2" libraryId="library-1" />);
    await vi.advanceTimersByTimeAsync(0);
    expect(api.clientBlob).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1000);
    expect(api.clientBlob).toHaveBeenCalledTimes(2);

    api.clientBlob.mockReset().mockRejectedValue(new Error('forbidden'));
    render(<MediaImage alt="Other" itemId="movie-3" libraryId="library-1" />);
    await vi.advanceTimersByTimeAsync(20_000);
    expect(api.clientBlob).toHaveBeenCalledTimes(1);
  } finally {
    vi.useRealTimers();
  }
});
