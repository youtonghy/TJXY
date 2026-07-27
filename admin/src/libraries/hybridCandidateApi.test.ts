import { apiRequest } from '../api/httpClient';
import {
  listHybridCandidates,
  pinHybridCandidate,
  unpinHybridCandidate,
} from './hybridCandidateApi';

vi.mock('../api/httpClient', async (importOriginal) => {
  const original = await importOriginal<typeof import('../api/httpClient')>();
  return { ...original, apiRequest: vi.fn() };
});

const requestMock = vi.mocked(apiRequest);
const libraryId = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11';
const itemId = '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12';

beforeEach(() => {
  requestMock.mockReset();
});

it('loads and strictly validates one stable candidate page', async () => {
  requestMock.mockResolvedValue({
    Items: [{
      Id: itemId,
      Name: 'Pinned Series',
      ProductionYear: 2026,
      StructureState: 'NotExpanded',
      SelectedAt: '2026-07-25T02:03:04Z',
    }],
    TotalRecordCount: 1,
    StartIndex: 0,
  });

  await expect(listHybridCandidates(libraryId, 0, 50)).resolves.toEqual({
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
  expect(requestMock).toHaveBeenCalledWith(
    `/Admin/Libraries/${libraryId}/HybridCandidates?StartIndex=0&Limit=50`,
    {},
  );

  requestMock.mockResolvedValue({ Items: [], TotalRecordCount: -1, StartIndex: 0 });
  await expect(listHybridCandidates(libraryId, 0, 50)).rejects.toMatchObject({
    category: 'invalid-response',
  });
});

it('uses idempotent pin and remove commands with validated identifiers', async () => {
  requestMock.mockResolvedValue(undefined);

  await pinHybridCandidate(libraryId, itemId);
  await unpinHybridCandidate(libraryId, itemId);

  const path = `/Admin/Libraries/${libraryId}/HybridCandidates/${itemId}`;
  expect(requestMock).toHaveBeenNthCalledWith(1, path, { method: 'PUT' });
  expect(requestMock).toHaveBeenNthCalledWith(2, path, { method: 'DELETE' });
  await expect(pinHybridCandidate('invalid', itemId)).rejects.toMatchObject({
    category: 'validation',
  });
});
