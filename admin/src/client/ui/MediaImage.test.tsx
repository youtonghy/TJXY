import { render, screen, waitFor } from '@testing-library/react';
import { MediaImage } from './MediaImage';

const api = vi.hoisted(() => ({ clientBlob: vi.fn() }));
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
