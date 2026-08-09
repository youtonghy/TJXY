import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';

import {
  AccessDeniedPage,
  ApplicationError,
  AuthenticationErrorPage,
  LoadingPage,
  NotFoundPage,
  PageError,
} from './SystemPages';

const logout = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));

vi.mock('ra-core', () => ({
  useLogout: () => logout,
}));

function renderInRouter(node: React.ReactNode) {
  return render(<MemoryRouter>{node}</MemoryRouter>);
}

describe('system pages', () => {
  beforeEach(() => logout.mockClear());

  it('renders a labeled loading state with one heading and a safe destination', () => {
    renderInRouter(<LoadingPage />);
    expect(screen.getAllByRole('heading', { level: 1 })).toHaveLength(1);
    expect(screen.getByRole('status')).toHaveTextContent('Preparing the admin workspace');
    expect(screen.getByRole('link', { name: 'Back to Users' })).toHaveAttribute(
      'href',
      '/admin/users',
    );
  });

  it('renders initial and application errors with explicit retry actions', async () => {
    const user = userEvent.setup();
    const retry = vi.fn();
    const reset = vi.fn();
    const { rerender } = renderInRouter(<PageError error={new Error('secret')} onRetry={retry} />);
    expect(screen.getAllByRole('heading', { level: 1 })).toHaveLength(1);
    await user.click(screen.getByRole('button', { name: 'Retry' }));
    expect(retry).toHaveBeenCalledOnce();

    rerender(
      <MemoryRouter>
        <ApplicationError
          error={new Error('render failure')}
          errorInfo={{ componentStack: 'private stack' }}
          resetErrorBoundary={reset}
        />
      </MemoryRouter>,
    );
    expect(screen.getAllByRole('heading', { level: 1 })).toHaveLength(1);
    expect(screen.queryByText('private stack')).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Try again' }));
    expect(reset).toHaveBeenCalledOnce();
  });

  it('does not sign out from Access Denied until the user explicitly asks', async () => {
    const user = userEvent.setup();
    renderInRouter(<AccessDeniedPage />);

    expect(screen.getAllByRole('heading', { level: 1 })).toHaveLength(1);
    expect(logout).not.toHaveBeenCalled();
    await user.click(screen.getByRole('button', { name: 'Sign out' }));
    expect(logout).toHaveBeenCalledWith({}, undefined, false);
  });

  it('offers useful routes from authentication failure and not found states', () => {
    const { rerender } = renderInRouter(<AuthenticationErrorPage />);
    expect(screen.getAllByRole('heading', { level: 1 })).toHaveLength(1);
    expect(screen.getByRole('link', { name: 'Go to sign in' })).toHaveAttribute(
      'href',
      '/app/login?redirect=%2Fadmin',
    );

    rerender(
      <MemoryRouter>
        <NotFoundPage />
      </MemoryRouter>,
    );
    expect(screen.getAllByRole('heading', { level: 1 })).toHaveLength(1);
    expect(screen.getByRole('link', { name: 'Back to Users' })).toHaveAttribute(
      'href',
      '/admin/users',
    );
  });
});
