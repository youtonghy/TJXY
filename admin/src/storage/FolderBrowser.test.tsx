import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { FolderBrowser } from './FolderBrowser';

const path = [
  { id: 'root', name: 'My Drive' },
  { id: 'shows', name: 'Shows' },
];

it('opens folders, navigates breadcrumbs, and loads more pages', async () => {
  const user = userEvent.setup();
  const onNavigate = vi.fn();
  const onOpen = vi.fn();
  const onLoadMore = vi.fn();
  render(
    <FolderBrowser
      ariaLabel="Google Drive folders"
      directories={[{ id: 'season', name: 'Season 1' }]}
      error={null}
      hasMore
      isDisabled={false}
      isLoading={false}
      isLoadingMore={false}
      onLoadMore={onLoadMore}
      onNavigate={onNavigate}
      onOpen={onOpen}
      onRetry={vi.fn()}
      path={path}
    />,
  );

  await user.click(screen.getByRole('button', { name: 'Open Season 1' }));
  expect(onOpen).toHaveBeenCalledWith({ id: 'season', name: 'Season 1' });
  await user.click(screen.getByRole('link', { name: 'My Drive' }));
  expect(onNavigate).toHaveBeenCalledWith(0);
  await user.click(screen.getByRole('button', { name: 'Load more folders' }));
  expect(onLoadMore).toHaveBeenCalledOnce();
  expect(screen.getByText('Shows', { selector: 'p' })).toBeVisible();
});

it('keeps the current folder explicit across loading, error, and empty continuation states', async () => {
  const retry = vi.fn();
  const { rerender } = render(
    <FolderBrowser
      ariaLabel="OneDrive folders"
      directories={[]}
      error={new Error('private-provider-detail')}
      hasMore
      isDisabled={false}
      isLoading={false}
      isLoadingMore={false}
      onLoadMore={vi.fn()}
      onNavigate={vi.fn()}
      onOpen={vi.fn()}
      onRetry={retry}
      path={[{ id: 'root', name: 'OneDrive' }]}
    />,
  );

  expect(screen.getByText('OneDrive', { selector: 'p' })).toBeVisible();
  expect(screen.getByText('The folder list could not be loaded', { selector: 'p' })).toBeVisible();
  expect(screen.queryByText('private-provider-detail')).not.toBeInTheDocument();
  await userEvent.setup().click(screen.getByRole('button', { name: 'Retry' }));
  expect(retry).toHaveBeenCalledOnce();

  rerender(
    <FolderBrowser
      ariaLabel="OneDrive folders"
      directories={[]}
      error={null}
      hasMore
      isDisabled={false}
      isLoading
      isLoadingMore={false}
      onLoadMore={vi.fn()}
      onNavigate={vi.fn()}
      onOpen={vi.fn()}
      onRetry={vi.fn()}
      path={[{ id: 'root', name: 'OneDrive' }]}
    />,
  );
  expect(screen.getByRole('status', { name: 'Loading folders' })).toBeVisible();

  rerender(
    <FolderBrowser
      ariaLabel="OneDrive folders"
      directories={[]}
      error={null}
      hasMore={false}
      isDisabled={false}
      isLoading={false}
      isLoadingMore={false}
      onLoadMore={vi.fn()}
      onNavigate={vi.fn()}
      onOpen={vi.fn()}
      onRetry={vi.fn()}
      path={[{ id: 'root', name: 'OneDrive' }]}
    />,
  );
  expect(screen.getByText('This folder has no child folders.')).toBeVisible();
  expect(within(screen.getByRole('region', { name: 'OneDrive folders' })).getByText('OneDrive', { selector: 'p' })).toBeVisible();
});
