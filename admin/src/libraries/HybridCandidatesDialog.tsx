import { DeleteOutlineOutlined, PushPinOutlined } from '@mui/icons-material';
import {
  Alert,
  Button,
  CircularProgress,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  IconButton,
  Stack,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TablePagination,
  TableRow,
  TextField,
  Tooltip,
} from '@mui/material';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useNotify } from 'react-admin';

import type { HybridCandidatePage } from './hybridCandidateApi';
import {
  listHybridCandidates,
  pinHybridCandidate,
  unpinHybridCandidate,
} from './hybridCandidateApi';
import type { LibraryOption } from './libraryApi';

const PAGE_SIZE = 50;
const EMPTY_PAGE: HybridCandidatePage = { items: [], totalRecordCount: 0, startIndex: 0 };
type CandidateLoadResult = { page: HybridCandidatePage } | { error: unknown };

export function HybridCandidatesDialog({
  library,
  onClose,
}: {
  library: LibraryOption;
  onClose: () => void;
}) {
  const notify = useNotify();
  const [pageIndex, setPageIndex] = useState(0);
  const [page, setPage] = useState<HybridCandidatePage>(EMPTY_PAGE);
  const [itemId, setItemId] = useState('');
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const requestSequence = useRef(0);

  const applyLoadResult = useCallback((sequence: number, result: CandidateLoadResult) => {
    if (sequence !== requestSequence.current) return;
    if ('page' in result) setPage(result.page);
    else notifyError(notify, result.error, 'Background candidates could not be loaded.');
    setLoading(false);
  }, [notify]);

  const reload = useCallback(async () => {
    const sequence = ++requestSequence.current;
    setLoading(true);
    applyLoadResult(sequence, await fetchCandidateResult(library.id, pageIndex));
  }, [applyLoadResult, library.id, pageIndex]);

  useEffect(() => {
    const abort = new AbortController();
    const sequence = ++requestSequence.current;
    void fetchCandidateResult(library.id, pageIndex, abort.signal).then((result) => {
      if (!abort.signal.aborted) applyLoadResult(sequence, result);
    });
    return () => { abort.abort(); };
  }, [applyLoadResult, library.id, pageIndex]);

  const pin = async () => {
    if (busy) return;
    setBusy(true);
    try {
      await pinHybridCandidate(library.id, itemId);
      setItemId('');
      notify('Background candidate pinned.', { type: 'success' });
      await reload();
    } catch (error: unknown) {
      notifyError(notify, error, 'The background candidate could not be pinned.');
    } finally {
      setBusy(false);
    }
  };

  const remove = async (candidateId: string) => {
    if (busy) return;
    setBusy(true);
    try {
      await unpinHybridCandidate(library.id, candidateId);
      notify('Background candidate pin removed.', { type: 'success' });
      await reload();
    } catch (error: unknown) {
      notifyError(notify, error, 'The background candidate pin could not be removed.');
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open
      fullWidth
      maxWidth="md"
      onClose={() => { if (!busy) onClose(); }}
    >
      <DialogTitle>Background candidates for {library.name}</DialogTitle>
      <DialogContent>
        <Stack spacing={2} sx={{ pt: 1 }}>
          <Stack direction={{ xs: 'column', sm: 'row' }} spacing={1}>
            <TextField
              fullWidth
              label="Catalog item ID"
              value={itemId}
              disabled={busy}
              onChange={(event) => { setItemId(event.target.value); }}
              slotProps={{ htmlInput: { maxLength: 36 } }}
            />
            <Button
              variant="contained"
              startIcon={busy ? <CircularProgress size={18} color="inherit" /> : <PushPinOutlined />}
              disabled={
                busy
                || !library.enabled
                || library.expansionPolicy !== 'background'
                || !validId(itemId.trim())
              }
              onClick={() => void pin()}
            >
              Pin candidate
            </Button>
          </Stack>

          <TableContainer>
            <Table aria-label="Pinned background candidates" sx={{ minWidth: 620 }}>
              <TableHead>
                <TableRow>
                  <TableCell>Name</TableCell>
                  <TableCell align="right">Year</TableCell>
                  <TableCell>Structure</TableCell>
                  <TableCell>Selected</TableCell>
                  <TableCell align="right">Actions</TableCell>
                </TableRow>
              </TableHead>
              <TableBody>
                {page.items.map((candidate) => (
                  <TableRow key={candidate.id} hover>
                    <TableCell component="th" scope="row">{candidate.name}</TableCell>
                    <TableCell align="right">{candidate.productionYear ?? '-'}</TableCell>
                    <TableCell>{candidate.structureState}</TableCell>
                    <TableCell>{new Date(candidate.selectedAt).toLocaleString()}</TableCell>
                    <TableCell align="right">
                      <Tooltip title={`Remove pin for ${candidate.name}`}>
                        <span>
                          <IconButton
                            aria-label={`Remove pin for ${candidate.name}`}
                            disabled={busy}
                            onClick={() => void remove(candidate.id)}
                          >
                            <DeleteOutlineOutlined />
                          </IconButton>
                        </span>
                      </Tooltip>
                    </TableCell>
                  </TableRow>
                ))}
                {!loading && page.items.length === 0 && (
                  <TableRow>
                    <TableCell colSpan={5}>
                      <Alert severity="info">No background candidates are pinned.</Alert>
                    </TableCell>
                  </TableRow>
                )}
              </TableBody>
            </Table>
            {loading && (
              <Stack sx={{ alignItems: 'center', py: 3 }}>
                <CircularProgress size={28} aria-label="Loading background candidates" />
              </Stack>
            )}
          </TableContainer>
          <TablePagination
            component="div"
            count={page.totalRecordCount}
            page={pageIndex}
            rowsPerPage={PAGE_SIZE}
            rowsPerPageOptions={[PAGE_SIZE]}
            onPageChange={(_, nextPage) => {
              setLoading(true);
              setPageIndex(nextPage);
            }}
            onRowsPerPageChange={() => { setPageIndex(0); }}
          />
        </Stack>
      </DialogContent>
      <DialogActions>
        <Button disabled={busy} onClick={onClose}>Close</Button>
      </DialogActions>
    </Dialog>
  );
}

function validId(value: string): boolean {
  return /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/iu.test(value);
}

async function fetchCandidateResult(
  libraryId: string,
  pageIndex: number,
  signal?: AbortSignal,
): Promise<CandidateLoadResult> {
  try {
    return {
      page: await listHybridCandidates(libraryId, pageIndex * PAGE_SIZE, PAGE_SIZE, signal),
    };
  } catch (error: unknown) {
    return { error };
  }
}

function notifyError(
  notify: ReturnType<typeof useNotify>,
  error: unknown,
  fallback: string,
): void {
  const message = error instanceof Error && error.message.length > 0 ? error.message : fallback;
  notify(message, { type: 'error' });
}
