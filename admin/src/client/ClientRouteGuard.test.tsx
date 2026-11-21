import { render, screen } from '@testing-library/react';
import { useEffect } from 'react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';

import { ClientRouteGuard } from './ClientApp';

const auth = vi.hoisted(() => {
  const state: { user: { Id: string; Name: string } | null } = { user: { Id: 'user-a', Name: 'A' } };
  return state;
});

vi.mock('./auth/ClientAuthContext', () => ({
  ClientAuthProvider: ({ children }: { children: unknown }) => children,
  useClientAuth: () => ({ user: auth.user, isLoading: false }),
}));
vi.mock('./api/streamRoutes', () => ({ ensureStreamRoutes: () => Promise.resolve() }));

const mounts = vi.fn();

function AccountPage() {
  useEffect(() => { mounts(); }, []);
  return <p>account page</p>;
}

function renderGuard() {
  return render(
    <MemoryRouter>
      <Routes>
        <Route element={<ClientRouteGuard />}>
          <Route element={<AccountPage />} index />
        </Route>
      </Routes>
    </MemoryRouter>,
  );
}

it('rebuilds protected pages when the signed-in account changes', () => {
  const view = renderGuard();
  expect(screen.getByText('account page')).toBeInTheDocument();
  expect(mounts).toHaveBeenCalledTimes(1);

  view.rerender(
    <MemoryRouter>
      <Routes>
        <Route element={<ClientRouteGuard />}>
          <Route element={<AccountPage />} index />
        </Route>
      </Routes>
    </MemoryRouter>,
  );
  expect(mounts).toHaveBeenCalledTimes(1);

  auth.user = { Id: 'user-b', Name: 'B' };
  view.rerender(
    <MemoryRouter>
      <Routes>
        <Route element={<ClientRouteGuard />}>
          <Route element={<AccountPage />} index />
        </Route>
      </Routes>
    </MemoryRouter>,
  );
  expect(mounts).toHaveBeenCalledTimes(2);
});
