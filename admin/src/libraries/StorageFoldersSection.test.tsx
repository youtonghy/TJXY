import { act, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { renderWithAdmin } from '../test/renderWithAdmin';
import type { LibraryOption } from './libraryApi';
import { listFolderContents, listLibraryFolders, type FolderContents } from './libraryFoldersApi';
import { StorageFoldersSection } from './StorageFoldersSection';

vi.mock('./libraryFoldersApi', () => ({ listLibraryFolders: vi.fn(), listFolderContents: vi.fn() }));
const library: LibraryOption = {
  id: 'library-1', name: 'Movies', collectionType: 'movies', locations: ['tjxy://storage-root/root-1'],
  enabled: true, scanProfile: 'Lazy', profileVersion: 1, objectSelectionScope: 'title_layer',
  metadataPolicy: 'basic', metadataSourceMode: 'automatic_scrape', localMetadataAccessMode: 'import',
  expansionPolicy: 'on_browse', probePolicy: 'on_playback',
};
const rootContents: FolderContents = { indexed: false, items: [
  { name: 'readme.txt', path: 'readme.txt', isDirectory: false, size: 1500, modifiedAt: null },
  { name: 'Movies', path: 'Movies', isDirectory: true, size: null, modifiedAt: null },
] };
const foldersMock = vi.mocked(listLibraryFolders);
const contentsMock = vi.mocked(listFolderContents);

beforeEach(() => {
  foldersMock.mockReset().mockResolvedValue([{ id: 'root-1', name: 'Media', path: '/mnt/media', provider: 'filesystem' }]);
  contentsMock.mockReset().mockResolvedValue(rootContents);
});

function renderFolders() {
  return renderWithAdmin(<StorageFoldersSection isPending={false} library={library} onOpen={vi.fn()} />, { strict: true });
}

it('shows real paths and counts, then previews folders before files and navigates back', async () => {
  renderFolders();
  const user = userEvent.setup();
  expect(screen.getByLabelText('Folder count')).toHaveTextContent('1');
  await user.click(await screen.findByRole('button', { name: 'Preview folder /mnt/media' }));
  const dialog = await screen.findByRole('dialog', { name: 'Folder contents' });
  expect(await within(dialog).findByText('1 folders · 1 files')).toBeVisible();
  expect(within(dialog).getByText('TXT · 1.5 KB')).toBeVisible();
  expect(within(dialog).getAllByRole('listitem')[0]).toHaveTextContent('Movies');
  contentsMock.mockResolvedValueOnce({ indexed: false, items: [] });
  await user.click(within(dialog).getByRole('button', { name: 'Open folder Movies' }));
  expect(await within(dialog).findByText('This folder is empty.')).toBeVisible();
  expect(within(dialog).getByText('/mnt/media/Movies')).toBeVisible();
  expect(contentsMock).toHaveBeenLastCalledWith('library-1', 'root-1', 'Movies', expect.any(AbortSignal));
  await user.click(within(dialog).getByRole('button', { name: 'Parent folder' }));
  expect(await within(dialog).findByText('readme.txt')).toBeVisible();
  await user.click(within(dialog).getByRole('button', { name: 'Close' }));
  await waitFor(() => { expect(screen.queryByRole('dialog')).not.toBeInTheDocument(); });
});

it('does not overwrite the current folder with a late response after returning to its parent', async () => {
  renderFolders();
  const user = userEvent.setup();
  await user.click(await screen.findByRole('button', { name: 'Preview folder /mnt/media' }));
  let finishChild: ((value: FolderContents) => void) | undefined;
  contentsMock.mockImplementationOnce(() => new Promise((resolve) => { finishChild = resolve; }));
  await user.click(await screen.findByRole('button', { name: 'Open folder Movies' }));
  await waitFor(() => { expect(contentsMock).toHaveBeenLastCalledWith('library-1', 'root-1', 'Movies', expect.any(AbortSignal)); });
  await user.click(screen.getByRole('button', { name: 'Parent folder' }));
  expect(await screen.findByText('readme.txt')).toBeVisible();
  await act(async () => { finishChild?.({ indexed: false, items: [] }); await Promise.resolve(); });
  expect(screen.getByText('readme.txt')).toBeVisible();
  expect(screen.queryByText('This folder is empty.')).not.toBeInTheDocument();
});

it('shows a retryable preview error and identifies synchronized cloud listings', async () => {
  foldersMock.mockResolvedValue([{ id: 'root-1', name: 'Cloud Movies', path: null, provider: 'google-drive' }]);
  contentsMock.mockRejectedValueOnce(new Error('unavailable')).mockResolvedValue({ indexed: true, items: [] });
  renderFolders();
  const user = userEvent.setup();
  await user.click(await screen.findByRole('button', { name: 'Preview folder Cloud Movies' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('This folder could not be read');
  await user.click(screen.getByRole('button', { name: 'Retry' }));
  expect(await screen.findByText(/Synchronized cloud inventory/u)).toBeVisible();
});

it('reports a directory limit without presenting a partial list as a total', async () => {
  contentsMock.mockRejectedValue({ status: 413 });
  renderFolders();
  const user = userEvent.setup();
  await user.click(await screen.findByRole('button', { name: 'Preview folder /mnt/media' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('10,000-entry preview limit');
  expect(screen.queryByText('0 folders · 0 files')).not.toBeInTheDocument();
});
