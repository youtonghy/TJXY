import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';

import { SystemLocaleProvider } from '../../settings/SystemLocaleProvider';
import { ClientLoginPage } from './ClientLoginPage';

const languageApi = vi.hoisted(() => ({
  getSystemLanguage: vi.fn(),
}));

vi.mock('../../settings/systemLanguageApi', () => ({
  getSystemLanguage: languageApi.getSystemLanguage,
  saveSystemLanguage: vi.fn(),
}));

vi.mock('./ClientAuthContext', () => ({
  useClientAuth: () => ({ signIn: vi.fn() }),
}));

beforeEach(() => {
  languageApi.getSystemLanguage.mockReset();
  languageApi.getSystemLanguage.mockResolvedValue({ locale: 'en-US', revision: 1, supportedLocales: ['zh-CN', 'en-US'] });
  window.localStorage.setItem('tjxy-system-locale', 'en-US');
});

it('uses the shared system mark in the client sign-in brand', () => {
  const { container } = render(
    <MemoryRouter initialEntries={['/app/login']}>
      <ClientLoginPage />
    </MemoryRouter>,
  );

  expect(screen.getByText('Your media library')).toBeVisible();
  expect(container.querySelector('img')).toHaveAttribute('src', '/brand/tjxy-mark.webp');
});

it('places the inline language selector at the top right and updates the login copy', async () => {
  const user = userEvent.setup();
  render(
    <SystemLocaleProvider>
      <MemoryRouter initialEntries={['/app/login']}>
        <ClientLoginPage />
      </MemoryRouter>
    </SystemLocaleProvider>,
  );

  const selector = await screen.findByRole('button', { name: /Interface language/ });
  expect(selector.parentElement).toHaveClass('absolute', 'right-5', 'top-5');
  await user.click(selector);
  await user.click(await screen.findByRole('option', { name: '中文' }));
  expect(await screen.findByRole('heading', { name: '欢迎回来' })).toBeVisible();
});
