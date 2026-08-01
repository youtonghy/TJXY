import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';

import { ClientLoginPage } from './ClientLoginPage';

vi.mock('./ClientAuthContext', () => ({
  useClientAuth: () => ({ signIn: vi.fn() }),
}));

it('uses the shared system mark in the client sign-in brand', () => {
  const { container } = render(
    <MemoryRouter initialEntries={['/app/login']}>
      <ClientLoginPage />
    </MemoryRouter>,
  );

  expect(screen.getByText('Your media library')).toBeVisible();
  expect(container.querySelector('img')).toHaveAttribute('src', '/brand/tjxy-mark.webp');
});
