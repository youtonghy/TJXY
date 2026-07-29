import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { AsyncContent } from './AsyncContent';

const content = <div>Loaded records</div>;
const loading = <div>Loading records</div>;
const empty = <div>No records yet</div>;

describe('AsyncContent', () => {
  it('uses the loading state only before data exists', () => {
    render(
      <AsyncContent
        isPending
        error={null}
        hasData={false}
        isEmpty={false}
        onRetry={vi.fn()}
        loading={loading}
        empty={empty}
      >
        {content}
      </AsyncContent>,
    );

    expect(screen.getByText('Loading records')).toBeVisible();
    expect(screen.queryByText('Loaded records')).not.toBeInTheDocument();
  });

  it('renders a retryable page error for an initial failure', async () => {
    const user = userEvent.setup();
    const onRetry = vi.fn();
    render(
      <AsyncContent
        isPending={false}
        error={new Error('private upstream detail')}
        hasData={false}
        isEmpty={false}
        onRetry={onRetry}
        loading={loading}
        empty={empty}
      >
        {content}
      </AsyncContent>,
    );

    expect(screen.getByRole('alert')).toHaveTextContent('Unable to load this content');
    expect(screen.queryByText('private upstream detail')).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Retry' }));
    expect(onRetry).toHaveBeenCalledOnce();
  });

  it('renders empty only after a successful empty response', () => {
    render(
      <AsyncContent
        isPending={false}
        error={null}
        hasData
        isEmpty
        onRetry={vi.fn()}
        loading={loading}
        empty={empty}
      >
        {content}
      </AsyncContent>,
    );

    expect(screen.getByText('No records yet')).toBeVisible();
    expect(screen.queryByText('Loaded records')).not.toBeInTheDocument();
  });

  it('retains stale data and offers retry when refresh fails', async () => {
    const user = userEvent.setup();
    const onRetry = vi.fn();
    render(
      <AsyncContent
        isPending={false}
        error={new Error('refresh failed')}
        hasData
        isEmpty={false}
        onRetry={onRetry}
        loading={loading}
        empty={empty}
      >
        {content}
      </AsyncContent>,
    );

    expect(screen.getByText('Loaded records')).toBeVisible();
    expect(screen.getByRole('alert')).toHaveTextContent('Showing the last available data');
    await user.click(screen.getByRole('button', { name: 'Retry refresh' }));
    expect(onRetry).toHaveBeenCalledOnce();
  });
});
