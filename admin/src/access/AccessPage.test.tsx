import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useLocation, useNavigate } from 'react-router-dom';

import { renderWithAdmin } from '../test/renderWithAdmin';
import { AccessPage } from './AccessPage';

vi.mock('./DevicesPanel', () => ({ DevicesPanel: () => <div>Device records</div> }));
vi.mock('./ApiKeysPanel', () => ({ ApiKeysPanel: () => <div>API key records</div> }));

function HistoryProbe() {
  const location = useLocation();
  const navigate = useNavigate();
  return (
    <div>
      <output aria-label="Current location">{`${location.pathname}${location.search}`}</output>
      <button onClick={() => { void navigate(-1); }} type="button">Back</button>
      <button onClick={() => { void navigate(1); }} type="button">Forward</button>
    </div>
  );
}

function renderAccess(initialEntry = '/admin/access') {
  renderWithAdmin(
    <>
      <AccessPage />
      <HistoryProbe />
    </>,
    { initialEntries: [initialEntry] },
  );
}

it('uses HeroUI tabs, URL state, and mounts only the active workflow', async () => {
  renderAccess();
  const user = userEvent.setup();

  const devices = screen.getByRole('tab', { name: 'Devices' });
  const apiKeys = screen.getByRole('tab', { name: 'API Keys' });
  expect(devices).toHaveAttribute('aria-selected', 'true');
  expect(devices.querySelector('svg')).toHaveAttribute('aria-hidden', 'true');
  expect(apiKeys.querySelector('svg')).toHaveAttribute('aria-hidden', 'true');
  expect(devices.closest('.tabs')).not.toHaveClass('tabs--secondary');
  expect(screen.getByText('Device records')).toBeVisible();
  expect(screen.queryByText('API key records')).not.toBeInTheDocument();

  await user.click(apiKeys);
  expect(apiKeys).toHaveAttribute('aria-selected', 'true');
  expect(screen.getByRole('tabpanel')).toHaveTextContent('API key records');
  expect(screen.queryByText('Device records')).not.toBeInTheDocument();
  expect(screen.getByRole('status', { name: 'Current location' })).toHaveTextContent(
    '/admin/access?tab=api-keys',
  );

  const panel = screen.getByRole('tabpanel');
  const labelledBy = panel.getAttribute('aria-labelledby');
  expect(labelledBy).not.toBeNull();
  expect(document.getElementById(labelledBy ?? '')).toBe(apiKeys);
});

it('restores the selected workflow through browser history', async () => {
  renderAccess('/admin/access?tab=api-keys');
  const user = userEvent.setup();

  expect(screen.getByRole('tab', { name: 'API Keys' })).toHaveAttribute('aria-selected', 'true');
  await user.click(screen.getByRole('tab', { name: 'Devices' }));
  expect(screen.getByRole('status', { name: 'Current location' })).toHaveTextContent('/admin/access');

  await user.click(screen.getByRole('button', { name: 'Back' }));
  expect(screen.getByRole('tab', { name: 'API Keys' })).toHaveAttribute('aria-selected', 'true');
  expect(screen.getByText('API key records')).toBeVisible();

  await user.click(screen.getByRole('button', { name: 'Forward' }));
  expect(screen.getByRole('tab', { name: 'Devices' })).toHaveAttribute('aria-selected', 'true');
});

it('defaults missing and invalid tab values to devices', () => {
  renderAccess('/admin/access?tab=unknown');

  expect(screen.getByRole('tab', { name: 'Devices' })).toHaveAttribute('aria-selected', 'true');
  expect(screen.getByText('Device records')).toBeVisible();
});

it('preserves unrelated query parameters when selecting either tab', async () => {
  renderAccess('/admin/access?source=audit');
  const user = userEvent.setup();

  await user.click(screen.getByRole('tab', { name: 'API Keys' }));
  expect(screen.getByRole('status', { name: 'Current location' })).toHaveTextContent(
    '/admin/access?source=audit&tab=api-keys',
  );

  await user.click(screen.getByRole('tab', { name: 'Devices' }));
  expect(screen.getByRole('status', { name: 'Current location' })).toHaveTextContent(
    '/admin/access?source=audit',
  );
});
