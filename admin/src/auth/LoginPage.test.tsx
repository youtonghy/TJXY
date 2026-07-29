import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { StrictMode } from 'react';
import { MemoryRouter } from 'react-router-dom';

import { LoginPage } from './LoginPage';
import { checkServerReadiness } from '../api/readiness';

const login = vi.hoisted(() => vi.fn());

vi.mock('ra-core', async (importOriginal) => {
  const original = await importOriginal<typeof import('ra-core')>();
  return { ...original, useLogin: () => login };
});

vi.mock('../api/readiness', () => ({
  checkServerReadiness: vi.fn(),
}));

const readinessMock = vi.mocked(checkServerReadiness);

beforeEach(() => {
  login.mockReset();
  readinessMock.mockReset();
  readinessMock.mockResolvedValue(true);
});

function renderLogin(state?: unknown) {
  return render(
    <StrictMode>
      <MemoryRouter initialEntries={[{ pathname: '/admin/login', state }]}>
        <LoginPage />
      </MemoryRouter>
    </StrictMode>,
  );
}

it('renders visible credentials, password reveal, and anchored branding', async () => {
  const user = userEvent.setup();
  renderLogin();

  expect(screen.getByRole('heading', { level: 1, name: 'Administrator sign in' })).toBeVisible();
  expect(screen.getByRole('textbox', { name: 'Username' })).toBeVisible();
  const password = screen.getByLabelText('Password');
  expect(password).toHaveAttribute('type', 'password');
  expect(screen.getByTestId('login-brand')).toHaveClass('lg:fixed');

  await user.click(screen.getByRole('button', { name: 'Show password' }));
  expect(password).toHaveAttribute('type', 'text');
  expect(screen.getByRole('button', { name: 'Hide password' })).toBeVisible();
});

it('starts one readiness request in Strict Mode and does not block sign in when unavailable', async () => {
  readinessMock.mockResolvedValue(false);
  renderLogin();

  expect(await screen.findByText('Server unavailable')).toBeVisible();
  expect(readinessMock).toHaveBeenCalledOnce();
  expect(screen.getByRole('button', { name: 'Sign in' })).toBeEnabled();
});

it('aborts the readiness request on unmount', async () => {
  let requestSignal: AbortSignal | undefined;
  readinessMock.mockImplementation((signal) => {
    requestSignal = signal;
    return new Promise(() => undefined);
  });
  const view = renderLogin();
  await waitFor(() => { expect(readinessMock).toHaveBeenCalledOnce(); });

  view.unmount();
  expect(requestSignal?.aborted).toBe(true);
});

it('prevents duplicate submission and clears the password after success', async () => {
  const user = userEvent.setup();
  let resolveLogin: (() => void) | undefined;
  login.mockImplementation(() => new Promise<void>((resolve) => { resolveLogin = resolve; }));
  renderLogin({ nextPathname: '/admin/tasks', nextSearch: '?view=recent' });

  await user.type(screen.getByRole('textbox', { name: 'Username' }), 'Ada');
  await user.type(screen.getByLabelText('Password'), 'correct horse');
  const submit = screen.getByRole('button', { name: 'Sign in' });
  await user.click(submit);
  await user.click(submit);

  expect(login).toHaveBeenCalledOnce();
  expect(login).toHaveBeenCalledWith(
    { username: 'Ada', password: 'correct horse' },
    '/admin/tasks?view=recent',
  );
  expect(submit).toBeDisabled();

  resolveLogin?.();
  await waitFor(() => { expect(screen.getByLabelText('Password')).toHaveValue(''); });
});

it('associates a safe inline failure with the form and preserves the username', async () => {
  const user = userEvent.setup();
  login.mockRejectedValue(new Error('correct horse raw secret'));
  renderLogin();

  const username = screen.getByRole('textbox', { name: 'Username' });
  await user.type(username, 'Ada');
  await user.type(screen.getByLabelText('Password'), 'correct horse');
  await user.click(screen.getByRole('button', { name: 'Sign in' }));

  const error = await screen.findByRole('alert');
  expect(error).toHaveTextContent('Sign in failed');
  expect(error).not.toHaveTextContent('correct horse');
  expect(screen.getByRole('form', { name: 'Administrator sign in' })).toHaveAttribute(
    'aria-describedby',
    error.id,
  );
  expect(username).toHaveValue('Ada');
});
