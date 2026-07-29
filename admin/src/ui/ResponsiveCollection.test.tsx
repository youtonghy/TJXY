import { render, screen } from '@testing-library/react';

import { ResponsiveCollection } from './ResponsiveCollection';
import { StatusChip, type StatusTone } from './StatusChip';

describe('ResponsiveCollection', () => {
  it('keeps labeled desktop and mobile representations behind stable CSS breakpoints', () => {
    render(
      <ResponsiveCollection
        ariaLabel="Users"
        desktop={<table><tbody><tr><td>Desktop user</td></tr></tbody></table>}
        mobile={<article>Mobile user</article>}
      />,
    );

    const representations = screen.getAllByRole('region', { name: 'Users' });
    expect(representations).toHaveLength(2);
    expect(representations[0]).toHaveClass('hidden', 'sm:block');
    expect(representations[1]).toHaveClass('block', 'sm:hidden');
  });
});

describe('StatusChip', () => {
  it.each<[StatusTone, string]>([
    ['neutral', 'chip--default'],
    ['accent', 'chip--accent'],
    ['success', 'chip--success'],
    ['warning', 'chip--warning'],
    ['danger', 'chip--danger'],
  ])('maps %s to a semantic HeroUI color while retaining visible text', (tone, colorClass) => {
    render(<StatusChip tone={tone}>Visible status</StatusChip>);

    const chip = screen.getByText('Visible status').closest('[data-slot="chip"]');
    expect(chip).toHaveClass(colorClass, 'chip--soft', 'chip--sm');
    expect(chip).toHaveTextContent('Visible status');
  });
});
