import { ThemeProvider } from '@mui/material/styles';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { theme } from '../theme';
import { HybridCandidatesDialog } from './HybridCandidatesDialog';
import {
  listHybridCandidates,
  pinHybridCandidate,
  unpinHybridCandidate,
} from './hybridCandidateApi';
import type { LibraryOption } from './libraryApi';

const notify = vi.fn();
vi.mock('react-admin', () => ({ useNotify: () => notify }));
vi.mock('./hybridCandidateApi', () => ({
  listHybridCandidates: vi.fn(),
  pinHybridCandidate: vi.fn(),
  unpinHybridCandidate: vi.fn(),
}));

const listMock = vi.mocked(listHybridCandidates);
const pinMock = vi.mocked(pinHybridCandidate);
const unpinMock = vi.mocked(unpinHybridCandidate);
const libraryId = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11';
const itemId = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12';
const library = {
  id: libraryId,
  name: 'Shows',
  collectionType: 'tvshows',
  locations: [],
  enabled: true,
  scanProfile: 'Hybrid',
  profileVersion: 1,
  objectSelectionScope: 'title_layer',
  metadataPolicy: 'basic',
  expansionPolicy: 'background',
  probePolicy: 'on_playback',
} satisfies LibraryOption;

beforeEach(() => {
  notify.mockReset();
  listMock.mockReset();
  pinMock.mockReset();
  unpinMock.mockReset();
  listMock.mockResolvedValue({
    items: [{
      id: itemId,
      name: 'Pinned Series',
      productionYear: 2026,
      structureState: 'NotExpanded',
      selectedAt: '2026-07-25T02:03:04Z',
    }],
    totalRecordCount: 1,
    startIndex: 0,
  });
  pinMock.mockResolvedValue(undefined);
  unpinMock.mockResolvedValue(undefined);
});

it('loads pins, adds a validated item, and removes only the future preference', async () => {
  render(
    <ThemeProvider theme={theme}>
      <HybridCandidatesDialog library={library} onClose={vi.fn()} />
    </ThemeProvider>,
  );
  const user = userEvent.setup();

  expect(await screen.findByRole('rowheader', { name: 'Pinned Series' })).toBeVisible();
  await user.type(screen.getByRole('textbox', { name: 'Catalog item ID' }), itemId);
  await user.click(screen.getByRole('button', { name: 'Pin candidate' }));
  expect(pinMock).toHaveBeenCalledWith(libraryId, itemId);
  await waitFor(() => { expect(listMock).toHaveBeenCalledTimes(2); });
  expect(notify).toHaveBeenCalledWith('Background candidate pinned.', { type: 'success' });

  await user.click(screen.getByRole('button', { name: 'Remove pin for Pinned Series' }));
  expect(unpinMock).toHaveBeenCalledWith(libraryId, itemId);
  await waitFor(() => { expect(listMock).toHaveBeenCalledTimes(3); });
  expect(notify).toHaveBeenCalledWith('Background candidate pin removed.', { type: 'success' });
});

it('ignores an older load that finishes after a pin reload', async () => {
  let resolveInitial: ((page: Awaited<ReturnType<typeof listHybridCandidates>>) => void) | undefined;
  listMock
    .mockImplementationOnce(() => new Promise((resolve) => { resolveInitial = resolve; }))
    .mockResolvedValueOnce({
      items: [{
        id: itemId,
        name: 'Latest pin',
        productionYear: null,
        structureState: 'NotExpanded',
        selectedAt: '2026-07-25T02:03:04Z',
      }],
      totalRecordCount: 1,
      startIndex: 0,
    });
  render(
    <ThemeProvider theme={theme}>
      <HybridCandidatesDialog library={library} onClose={vi.fn()} />
    </ThemeProvider>,
  );
  const user = userEvent.setup();

  await user.type(screen.getByRole('textbox', { name: 'Catalog item ID' }), itemId);
  await user.click(screen.getByRole('button', { name: 'Pin candidate' }));
  expect(await screen.findByRole('rowheader', { name: 'Latest pin' })).toBeVisible();

  act(() => {
    resolveInitial?.({ items: [], totalRecordCount: 0, startIndex: 0 });
  });
  await waitFor(() => {
    expect(screen.getByRole('rowheader', { name: 'Latest pin' })).toBeVisible();
  });
});

it('keeps dormant pins manageable but disables new pins outside an enabled background policy', async () => {
  render(
    <ThemeProvider theme={theme}>
      <HybridCandidatesDialog
        library={{ ...library, enabled: false, expansionPolicy: 'on_browse' }}
        onClose={vi.fn()}
      />
    </ThemeProvider>,
  );
  const user = userEvent.setup();

  expect(await screen.findByRole('rowheader', { name: 'Pinned Series' })).toBeVisible();
  await user.type(screen.getByRole('textbox', { name: 'Catalog item ID' }), itemId);
  expect(screen.getByRole('button', { name: 'Pin candidate' })).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Remove pin for Pinned Series' })).toBeEnabled();
});
