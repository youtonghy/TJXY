import { ThemeProvider } from '@mui/material/styles';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { theme } from '../theme';
import { AccessPage } from './AccessPage';

vi.mock('react-admin', () => ({
  Title: ({ title }: { title: string }) => <title>{title}</title>,
}));
vi.mock('./DevicesPanel', () => ({ DevicesPanel: () => <div>Device records</div> }));
vi.mock('./ApiKeysPanel', () => ({ ApiKeysPanel: () => <div>API key records</div> }));

it('uses accessible controlled tabs and mounts only the active workflow', async () => {
  render(<ThemeProvider theme={theme}><AccessPage /></ThemeProvider>);
  const user = userEvent.setup();

  const devices = screen.getByRole('tab', { name: 'Devices' });
  const apiKeys = screen.getByRole('tab', { name: 'API Keys' });
  expect(devices).toHaveAttribute('aria-selected', 'true');
  expect(devices).toHaveAttribute('aria-controls', 'access-panel-devices');
  expect(screen.getByRole('tabpanel')).toHaveAttribute('aria-labelledby', 'access-tab-devices');
  expect(screen.getByText('Device records')).toBeVisible();
  expect(screen.queryByText('API key records')).not.toBeInTheDocument();

  await user.click(apiKeys);
  expect(apiKeys).toHaveAttribute('aria-selected', 'true');
  expect(screen.getByRole('tabpanel')).toHaveAttribute('aria-labelledby', 'access-tab-api-keys');
  expect(screen.getByText('API key records')).toBeVisible();
  expect(screen.queryByText('Device records')).not.toBeInTheDocument();
});
