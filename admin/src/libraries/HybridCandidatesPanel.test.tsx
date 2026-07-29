import { Toast } from '@heroui/react';
import { act, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { useLocation } from 'react-router-dom';

import { defaultTestAuthProvider, renderWithAdmin } from '../test/renderWithAdmin';
import { AdminNotifications } from '../ui/AdminNotifications';
import { HybridCandidatesPanel } from './HybridCandidatesPanel';
import {
  listHybridCandidates,
  pinHybridCandidate,
  unpinHybridCandidate,
  type HybridCandidatePage,
} from './hybridCandidateApi';
import type { LibraryOption } from './libraryApi';

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
const secondItemId = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f13';
const otherLibraryId = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f14';
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
const candidate = {
  id: itemId,
  name: 'Pinned Series',
  productionYear: 2026,
  structureState: 'NotExpanded',
  selectedAt: '2026-07-25T02:03:04Z',
};
const firstPage: HybridCandidatePage = {
  items: [candidate],
  totalRecordCount: 1,
  startIndex: 0,
};

function renderCandidates(
  selectedLibrary: LibraryOption = library,
  authProvider = defaultTestAuthProvider,
) {
  return renderWithAdmin(
    <>
      <HybridCandidatesPanel library={selectedLibrary} />
      <AdminNotifications />
      <CurrentRoute />
    </>,
    { authProvider, initialEntries: [`/admin/libraries/${selectedLibrary.id}`], strict: true },
  );
}

function CurrentRoute() {
  const location = useLocation();
  return <span data-testid="current-route">{location.pathname}</span>;
}

function CandidateLibrarySwitcher() {
  const [selectedLibrary, setSelectedLibrary] = useState(library);
  return (
    <>
      <button
        onClick={() => { setSelectedLibrary({ ...library, id: otherLibraryId, name: 'Other library' }); }}
        type="button"
      >
        Switch candidate library
      </button>
      <HybridCandidatesPanel library={selectedLibrary} />
    </>
  );
}

async function candidatesGrid() {
  return await screen.findByRole('grid', { name: 'Pinned background candidates' });
}

beforeEach(() => {
  listMock.mockReset();
  pinMock.mockReset();
  unpinMock.mockReset();
  listMock.mockResolvedValue(firstPage);
  pinMock.mockResolvedValue(undefined);
  unpinMock.mockResolvedValue(undefined);
});

afterEach(() => { vi.restoreAllMocks(); });

it('loads pins, adds a validated item, and removes only the future preference', async () => {
  const successToast = vi.spyOn(Toast.toast, 'success').mockReturnValue('candidate-success');
  renderCandidates();
  const user = userEvent.setup();
  let grid = await candidatesGrid();
  expect(within(grid).getByText('Not Expanded')).toBeVisible();

  await user.type(screen.getByRole('textbox', { name: 'Catalog item ID' }), secondItemId);
  await user.click(screen.getByRole('button', { name: 'Pin candidate' }));
  expect(pinMock).toHaveBeenCalledWith(libraryId, secondItemId);
  await waitFor(() => { expect(listMock).toHaveBeenCalledTimes(2); });
  expect(successToast).toHaveBeenCalledWith('Background candidate pinned.', expect.any(Object));

  grid = await candidatesGrid();
  await user.click(within(grid).getByRole('button', { name: 'Remove pin for Pinned Series' }));
  expect(unpinMock).toHaveBeenCalledWith(libraryId, itemId);
  await waitFor(() => { expect(listMock).toHaveBeenCalledTimes(3); });
  expect(successToast).toHaveBeenCalledWith('Background candidate pin removed.', expect.any(Object));
});

it('never calls candidate APIs for an incompatible library identifier', () => {
  renderCandidates({ ...library, id: 'opaque-library-id' });

  expect(screen.getByRole('heading', { name: 'Background candidates' })).toBeVisible();
  expect(screen.getByText('Candidate management unavailable')).toBeVisible();
  expect(listMock).not.toHaveBeenCalled();
  expect(pinMock).not.toHaveBeenCalled();
});

it('keeps dormant pins removable but pauses new pins outside an enabled background policy', async () => {
  renderCandidates({ ...library, enabled: false, expansionPolicy: 'on_browse' });
  const user = userEvent.setup();
  const grid = await candidatesGrid();

  await user.type(screen.getByRole('textbox', { name: 'Catalog item ID' }), secondItemId);
  expect(screen.getByRole('button', { name: 'Pin candidate' })).toBeDisabled();
  expect(within(grid).getByRole('button', { name: 'Remove pin for Pinned Series' })).toBeEnabled();
  expect(screen.getByText('New pins are paused')).toBeVisible();
});

it('requests fixed 50-row pages and exposes stable pagination controls', async () => {
  listMock
    .mockResolvedValueOnce({ ...firstPage, totalRecordCount: 51 })
    .mockResolvedValueOnce({
      items: [{ ...candidate, id: secondItemId, name: 'Final Series' }],
      totalRecordCount: 51,
      startIndex: 50,
    });
  renderCandidates();
  const user = userEvent.setup();
  await candidatesGrid();

  await user.click(screen.getByRole('button', { name: 'Next candidate page' }));

  await waitFor(() => {
    expect(listMock).toHaveBeenLastCalledWith(libraryId, 50, 50, expect.any(AbortSignal));
  });
  const grid = await candidatesGrid();
  expect(within(grid).getByText('Final Series')).toBeVisible();
  expect(screen.getByRole('button', { name: 'Candidate page 2' })).toBeVisible();
});

it('removes old candidate actions while a different library is loading', async () => {
  let finishOtherLoad: ((page: HybridCandidatePage) => void) | undefined;
  listMock
    .mockResolvedValueOnce(firstPage)
    .mockReturnValueOnce(new Promise((resolve) => { finishOtherLoad = resolve; }));
  renderWithAdmin(
    <CandidateLibrarySwitcher />,
    { initialEntries: [`/admin/libraries/${libraryId}`], strict: true },
  );
  const user = userEvent.setup();
  await candidatesGrid();

  await user.click(screen.getByRole('button', { name: 'Switch candidate library' }));

  expect(screen.getByRole('status', { name: 'Loading background candidates' })).toBeVisible();
  expect(screen.queryByRole('button', { name: 'Remove pin for Pinned Series' })).not.toBeInTheDocument();
  finishOtherLoad?.({
    items: [{ ...candidate, id: secondItemId, name: 'Other pin' }],
    totalRecordCount: 1,
    startIndex: 0,
  });
  const grid = await candidatesGrid();
  await user.click(within(grid).getByRole('button', { name: 'Remove pin for Other pin' }));
  expect(unpinMock).toHaveBeenCalledWith(otherLibraryId, secondItemId);
});

it('discards an older page response after a pin-triggered reload wins', async () => {
  let resolveInitial: ((page: HybridCandidatePage) => void) | undefined;
  listMock
    .mockImplementationOnce(() => new Promise((resolve) => { resolveInitial = resolve; }))
    .mockResolvedValueOnce({
      items: [{ ...candidate, name: 'Latest pin' }],
      totalRecordCount: 1,
      startIndex: 0,
    });
  renderCandidates();
  const user = userEvent.setup();
  await waitFor(() => { expect(listMock).toHaveBeenCalledOnce(); });

  await user.type(screen.getByRole('textbox', { name: 'Catalog item ID' }), secondItemId);
  await user.click(screen.getByRole('button', { name: 'Pin candidate' }));
  const grid = await candidatesGrid();
  expect(within(grid).getByText('Latest pin')).toBeVisible();

  act(() => { resolveInitial?.({ items: [], totalRecordCount: 0, startIndex: 0 }); });
  await waitFor(() => { expect(within(grid).getByText('Latest pin')).toBeVisible(); });
});

it('moves back to the last valid page after removing its final pin', async () => {
  listMock
    .mockResolvedValueOnce({ ...firstPage, totalRecordCount: 51 })
    .mockResolvedValueOnce({
      items: [{ ...candidate, id: secondItemId, name: 'Final Series' }],
      totalRecordCount: 51,
      startIndex: 50,
    })
    .mockResolvedValueOnce(firstPage);
  renderCandidates();
  const user = userEvent.setup();
  await candidatesGrid();
  await user.click(screen.getByRole('button', { name: 'Next candidate page' }));
  let grid = await candidatesGrid();
  await waitFor(() => { expect(within(grid).getByText('Final Series')).toBeVisible(); });

  await user.click(within(grid).getByRole('button', { name: 'Remove pin for Final Series' }));

  await waitFor(() => {
    expect(listMock).toHaveBeenLastCalledWith(libraryId, 0, 50, expect.any(AbortSignal));
  });
  grid = await candidatesGrid();
  expect(within(grid).getByText('Pinned Series')).toBeVisible();
  expect(screen.getByRole('button', { name: 'Candidate page 1' })).toBeVisible();
});

it('returns to the last valid page when a refresh observes a smaller total', async () => {
  listMock
    .mockResolvedValueOnce({ ...firstPage, totalRecordCount: 51 })
    .mockResolvedValueOnce({
      items: [{ ...candidate, id: secondItemId, name: 'Final Series' }],
      totalRecordCount: 51,
      startIndex: 50,
    })
    .mockResolvedValueOnce({ items: [], totalRecordCount: 50, startIndex: 50 })
    .mockResolvedValueOnce({ ...firstPage, totalRecordCount: 50 });
  renderCandidates();
  const user = userEvent.setup();
  await candidatesGrid();
  await user.click(screen.getByRole('button', { name: 'Next candidate page' }));
  await waitFor(() => {
    expect(screen.getByRole('button', { name: 'Candidate page 2' })).toBeVisible();
  });

  await user.click(screen.getByRole('button', { name: 'Reload background candidates' }));

  await waitFor(() => {
    expect(listMock).toHaveBeenLastCalledWith(libraryId, 0, 50, expect.any(AbortSignal));
  });
  const grid = await candidatesGrid();
  expect(within(grid).getByText('Pinned Series')).toBeVisible();
  expect(screen.getByRole('button', { name: 'Candidate page 1' })).toBeVisible();
});

it('keeps a candidate mutation pending until its authoritative reload settles', async () => {
  let finishReload: ((page: HybridCandidatePage) => void) | undefined;
  listMock
    .mockResolvedValueOnce(firstPage)
    .mockReturnValueOnce(new Promise((resolve) => { finishReload = resolve; }));
  renderCandidates();
  const user = userEvent.setup();
  const grid = await candidatesGrid();
  const removeButton = within(grid).getByRole('button', { name: 'Remove pin for Pinned Series' });

  await user.click(removeButton);
  await waitFor(() => { expect(listMock).toHaveBeenCalledTimes(2); });
  expect(removeButton).toHaveAttribute('data-pending', 'true');
  expect(screen.getByRole('textbox', { name: 'Catalog item ID' })).toBeDisabled();
  await user.click(removeButton);
  expect(unpinMock).toHaveBeenCalledOnce();

  finishReload?.({ items: [], totalRecordCount: 0, startIndex: 0 });
  expect(await screen.findByText('No background candidates are pinned.')).toBeVisible();
});

it('keeps existing pins with inline stale feedback after a failed refresh', async () => {
  const dangerToast = vi.spyOn(Toast.toast, 'danger').mockReturnValue('unexpected-toast');
  listMock
    .mockResolvedValueOnce(firstPage)
    .mockRejectedValueOnce(new Error('private-candidate-refresh-detail'));
  renderCandidates();
  const user = userEvent.setup();
  const grid = await candidatesGrid();

  await user.click(screen.getByRole('button', { name: 'Reload background candidates' }));

  expect(await screen.findByText('Showing the last available data')).toBeVisible();
  expect(within(grid).getByText('Pinned Series')).toBeVisible();
  expect(screen.queryByText('private-candidate-refresh-detail')).not.toBeInTheDocument();
  expect(dangerToast).not.toHaveBeenCalled();
});

it('reports a safe mutation failure and routes authorization failures before local feedback', async () => {
  const dangerToast = vi.spyOn(Toast.toast, 'danger').mockReturnValue('candidate-error');
  const checkError = vi.fn()
    .mockResolvedValueOnce(undefined)
    .mockRejectedValueOnce({
      logoutUser: false,
      message: false,
      redirectTo: '/admin/access-denied',
    });
  unpinMock.mockRejectedValueOnce(new Error('private-unpin-detail'));
  pinMock.mockRejectedValueOnce({ status: 403, message: 'private-pin-auth-detail' });
  renderCandidates(library, { ...defaultTestAuthProvider, checkError });
  const user = userEvent.setup();
  const grid = await candidatesGrid();

  await user.click(within(grid).getByRole('button', { name: 'Remove pin for Pinned Series' }));
  await waitFor(() => {
    expect(dangerToast).toHaveBeenCalledWith(
      'The background candidate change could not be completed.',
      expect.any(Object),
    );
  });
  expect(screen.queryByText('private-unpin-detail')).not.toBeInTheDocument();

  await user.type(screen.getByRole('textbox', { name: 'Catalog item ID' }), secondItemId);
  await user.click(screen.getByRole('button', { name: 'Pin candidate' }));
  await waitFor(() => { expect(screen.getByTestId('current-route')).toHaveTextContent('/admin/access-denied'); });
  expect(screen.queryByText('private-pin-auth-detail')).not.toBeInTheDocument();
  expect(dangerToast).toHaveBeenCalledTimes(1);
});
