import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { FolderPickerDialog } from './FolderPickerDialog';
import { listFilesystemDirectories, listFilesystemRoots } from './filesystemApi';

vi.mock('./filesystemApi', () => ({
  listFilesystemDirectories: vi.fn(),
  listFilesystemRoots: vi.fn(),
}));

const rootsMock = vi.mocked(listFilesystemRoots);
const directoriesMock = vi.mocked(listFilesystemDirectories);

beforeEach(() => {
  rootsMock.mockReset();
  directoriesMock.mockReset();
  rootsMock.mockResolvedValue([{ id: 'root-1', name: 'Media' }]);
  directoriesMock
    .mockResolvedValueOnce([{ name: 'Movies', relativePath: 'Movies', modifiedAt: null }])
    .mockResolvedValueOnce([]);
});

it('navigates with the Pro list view and returns an opaque folder selection', async () => {
  const onSelect = vi.fn();
  const onClose = vi.fn();
  render(
    <FolderPickerDialog isOpen onClose={onClose} onSelect={onSelect} />,
  );
  const user = userEvent.setup();

  expect(await screen.findByRole('treegrid', { name: 'Server folder tree' })).toBeVisible();
  expect(await screen.findByRole('grid', { name: 'Folder list view' })).toBeVisible();
  await user.click(screen.getByText('Movies', { selector: 'span' }));
  await waitFor(() => {
    expect(directoriesMock).toHaveBeenLastCalledWith('root-1', 'Movies', undefined);
  });
  await user.click(screen.getByRole('button', { name: 'Select folder' }));

  expect(onSelect).toHaveBeenCalledWith(
    { rootId: 'root-1', relativePath: 'Movies' },
    'Media / Movies',
  );
  expect(onClose).toHaveBeenCalledOnce();
});

it('reloads the server roots when the initial request fails', async () => {
  rootsMock
    .mockRejectedValueOnce(new Error('unavailable'))
    .mockResolvedValueOnce([{ id: 'root-1', name: 'Media' }]);
  directoriesMock.mockReset();
  directoriesMock.mockResolvedValue([]);
  render(
    <FolderPickerDialog isOpen onClose={vi.fn()} onSelect={vi.fn()} />,
  );
  const user = userEvent.setup();

  await user.click(await screen.findByRole('button', { name: 'Retry' }));

  await waitFor(() => {
    expect(rootsMock).toHaveBeenCalledTimes(2);
    expect(directoriesMock).toHaveBeenCalledWith('root-1', '', undefined);
  });
  expect(screen.getByRole('button', { name: 'Select folder' })).toBeEnabled();
});
