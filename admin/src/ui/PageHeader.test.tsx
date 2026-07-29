import { Button } from '@heroui/react';
import { render, screen, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';

import { PageHeader } from './PageHeader';

describe('PageHeader', () => {
  it('renders a single page title, semantic breadcrumbs, and a stable action slot', () => {
    render(
      <MemoryRouter>
        <PageHeader
          title="Edit user"
          description="Change identity and access settings."
          breadcrumbs={[
            { label: 'Users', to: '/admin/users' },
            { label: 'Edit user' },
          ]}
          actions={<Button>Save changes</Button>}
        />
      </MemoryRouter>,
    );

    expect(screen.getAllByRole('heading', { level: 1 })).toHaveLength(1);
    expect(screen.getByRole('heading', { level: 1, name: 'Edit user' })).toBeVisible();
    expect(document.title).toBe('Edit user | TJXY Admin');

    const breadcrumbs = screen.getByRole('navigation', { name: 'Breadcrumb' });
    expect(within(breadcrumbs).getByRole('link', { name: 'Users' })).toHaveAttribute(
      'href',
      '/admin/users',
    );
    expect(within(breadcrumbs).getByText('Edit user')).not.toHaveAttribute('href');

    const actions = screen.getByTestId('page-header-actions');
    expect(actions).toHaveClass('min-h-9', 'flex-wrap');
    expect(within(actions).getByRole('button', { name: 'Save changes' })).toBeEnabled();
  });
});
