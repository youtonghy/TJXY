import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';

import { IptvPage } from './IptvPage';

const iptvApi = vi.hoisted(() => ({
  isIptvSupportedShell: vi.fn(() => true),
}));

vi.mock('./iptvApi', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./iptvApi')>()),
  isIptvSupportedShell: iptvApi.isIptvSupportedShell,
}));

vi.mock('./iptvEpg', () => ({
  loadIptvGuide: vi.fn(() => Promise.resolve(new Map([['CCTV1', { start: 0, stop: 1, title: '晚间新闻' }]]))),
}));

beforeEach(() => {
  iptvApi.isIptvSupportedShell.mockReturnValue(true);
});

it('lists CCTV channels with links and metadata', async () => {
  render(<MemoryRouter><IptvPage /></MemoryRouter>);

  expect(await screen.findByRole('link', { name: /CCTV-1 综合/ }))
    .toHaveAttribute('href', '/app/iptv/cctv1');
  expect(screen.getByRole('link', { name: /CCTV-4K 超高清/ })).toBeInTheDocument();
  expect(await screen.findByText(/晚间新闻/)).toBeInTheDocument();
  expect(screen.getAllByText('FHD').length).toBeGreaterThan(0);
  expect(screen.getAllByText('7-day replay').length).toBeGreaterThan(0);
});

it('switches groups via tabs', async () => {
  render(<MemoryRouter><IptvPage /></MemoryRouter>);
  await screen.findByRole('link', { name: /CCTV-1 综合/ });

  const satellite = screen.getByRole('tab', { name: 'Satellite' });
  satellite.click();
  expect(await screen.findByRole('link', { name: /北京卫视/ })).toBeInTheDocument();
  expect(screen.queryByRole('link', { name: /CCTV-1 综合/ })).not.toBeInTheDocument();

  screen.getByRole('tab', { name: 'Digital' }).click();
  expect(await screen.findByRole('link', { name: /国学频道/ })).toBeInTheDocument();
});

it('shows a notice instead of channels in a plain browser', () => {
  iptvApi.isIptvSupportedShell.mockReturnValue(false);
  render(<MemoryRouter><IptvPage /></MemoryRouter>);
  expect(screen.getByText('IPTV needs the app')).toBeInTheDocument();
  expect(screen.queryByRole('link', { name: /CCTV-1/ })).not.toBeInTheDocument();
});
