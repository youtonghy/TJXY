import { screen } from '@testing-library/react';

import { renderWithAdmin } from '../test/renderWithAdmin';
import { AboutPage } from './AboutPage';

it('presents product and build information as a responsive card grid', () => {
  const { container } = renderWithAdmin(<AboutPage />, {
    initialEntries: ['/admin/settings/about'],
  });

  expect(screen.getByRole('heading', { name: 'TJXY', level: 2 })).toBeVisible();
  expect(container.querySelectorAll('.card')).toHaveLength(5);
  expect(screen.getByText('youtonghy')).toBeVisible();
  expect(screen.getByText('0.0.0')).toBeVisible();

  const repository = screen.getByRole('link', { name: /github\.com\/youtonghy\/TJXY/u });
  expect(repository).toHaveAttribute('href', 'https://github.com/youtonghy/TJXY');
  expect(repository).toHaveAttribute('target', '_blank');
});
