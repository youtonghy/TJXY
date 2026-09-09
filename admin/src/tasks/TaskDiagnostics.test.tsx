import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { renderWithAdmin } from '../test/renderWithAdmin';
import { TaskDiagnostics } from './TaskDiagnostics';
import { chooseNfo, getScanReport, listNfoChoices, listScanHistory, retryScanIssues, type NfoChoice } from './taskDiagnosticsApi';
import type { TaskJob } from './taskApi';

vi.mock('./taskDiagnosticsApi', () => ({ chooseNfo: vi.fn(), getScanReport: vi.fn(), listNfoChoices: vi.fn(), listScanHistory: vi.fn(), retryScanIssues: vi.fn() }));

const item = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11';
const choice: NfoChoice = { ItemId: item, RootId: item, Fingerprint: 'a'.repeat(64),
  Candidates: [{ Id: item, Name: 'Arrival - 4K.nfo', Digest: 'b'.repeat(64) }],
  ConflictFields: ['provider_ids'], SelectedObjectId: null, Status: 'NeedsSelection' };
const job: TaskJob = { id: item, taskKind: 'FullMediaScan', scopeType: 'Library', scopeId: item,
  status: 'Completed', priority: 20, attemptCount: 1, createdAt: null, startedAt: null, completedAt: null, outcome: 'CompletedWithWarnings' };

it('loads actionable candidates and submits only the explicitly chosen source', async () => {
  vi.mocked(listNfoChoices).mockResolvedValue([choice]);
  vi.mocked(chooseNfo).mockResolvedValue();
  const user = userEvent.setup();
  renderWithAdmin(<TaskDiagnostics jobs={[]} />);
  expect(listNfoChoices).not.toHaveBeenCalled();
  await user.click(screen.getByRole('button', { name: 'Load NFO choices' }));
  expect(await screen.findByText(/provider_ids/u)).toBeVisible();
  await user.click(screen.getByRole('button', { name: 'Choose Arrival - 4K.nfo' }));
  await waitFor(() => { expect(chooseNfo).toHaveBeenCalledWith(choice, item); });
  expect(await screen.findByRole('status')).toHaveTextContent('Choice saved');
});

it('shows accurate counts and leaves unresolved choices out of the retry action', async () => {
  vi.mocked(getScanReport).mockResolvedValue({ Counters: { items: 20, success: 18, failed: 1, skipped: 0, needs_selection: 1 },
    HasMore: false, Issues: [
      { ItemId: item, ChildJobId: item, ScopeId: item, ScopeType: 'CatalogItem', TaskKind: 'ResolveMetadata', NeedsSelection: false },
    ] });
  vi.mocked(retryScanIssues).mockResolvedValue(1);
  const user = userEvent.setup();
  renderWithAdmin(<TaskDiagnostics jobs={[job]} />);
  await user.click(screen.getByRole('button', { name: 'Load report' }));
  expect(await screen.findByText(/Succeeded 18/u)).toHaveTextContent('Needs selection 1');
  await user.click(screen.getByRole('button', { name: 'Retry failed items on this page' }));
  await waitFor(() => { expect(retryScanIssues).toHaveBeenCalledWith(item, 0); });
  expect(await screen.findByRole('status')).toHaveTextContent('1 failed items submitted');
});

it('does not offer retries for a page consisting only of unresolved NFO choices', async () => {
  vi.mocked(getScanReport).mockResolvedValue({ Counters: null, HasMore: false, Issues: [
    { ItemId: item, ChildJobId: item, ScopeId: item, ScopeType: 'CatalogItem', TaskKind: 'ResolveMetadata', NeedsSelection: true },
  ] });
  const user = userEvent.setup();
  renderWithAdmin(<TaskDiagnostics jobs={[job]} />);
  await user.click(screen.getByRole('button', { name: 'Load report' }));
  expect(await screen.findByText(/Detailed counts are unavailable/u)).toBeVisible();
  expect(screen.getByRole('button', { name: 'Retry failed items on this page' })).toBeDisabled();
});


it('finds scan reports after child tasks displace them from recent jobs', async () => {
  vi.mocked(listScanHistory).mockResolvedValue([{ Id: item, CreatedAt: '2026-09-09T00:00:00Z', State: 'Completed' }]);
  vi.mocked(getScanReport).mockResolvedValue({ Counters: { items: 1, success: 1, failed: 0, skipped: 0, needs_selection: 0 }, Issues: [], HasMore: false });
  const user = userEvent.setup();
  renderWithAdmin(<TaskDiagnostics jobs={[]} />);
  await user.click(screen.getByRole('button', { name: 'Load scan history' }));
  await user.click(await screen.findByRole('button', { name: 'Load report' }));
  expect(await screen.findByText(/Succeeded 1/u)).toBeVisible();
  expect(getScanReport).toHaveBeenCalledWith(item, 0, expect.any(AbortSignal));
});
