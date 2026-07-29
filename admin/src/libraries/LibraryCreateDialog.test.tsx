import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { LibraryCreateDialog } from './LibraryCreateDialog';

it('submits the approved defaults and resets the draft after success', async () => {
  const onCreate = vi.fn().mockResolvedValue(true);
  const onClose = vi.fn();
  const view = render(
    <LibraryCreateDialog isOpen isPending={false} onClose={onClose} onCreate={onCreate} />,
  );
  const user = userEvent.setup();

  await user.type(screen.getByRole('textbox', { name: 'Library name' }), '  Shows  ');
  await user.click(screen.getByRole('button', { name: 'Create library' }));

  expect(onCreate).toHaveBeenCalledWith({
    name: 'Shows',
    collectionType: 'mixed',
    enabled: true,
    scanProfile: 'Lazy',
  });

  view.rerender(
    <LibraryCreateDialog isOpen={false} isPending={false} onClose={onClose} onCreate={onCreate} />,
  );
  view.rerender(
    <LibraryCreateDialog isOpen isPending={false} onClose={onClose} onCreate={onCreate} />,
  );
  expect(screen.getByRole('textbox', { name: 'Library name' })).toHaveValue('');
});

it('preserves all input after failure and exposes HeroUI pending semantics', async () => {
  const onCreate = vi.fn().mockResolvedValue(false);
  const onClose = vi.fn();
  const view = render(
    <LibraryCreateDialog isOpen isPending={false} onClose={onClose} onCreate={onCreate} />,
  );
  const user = userEvent.setup();

  const name = screen.getByRole('textbox', { name: 'Library name' });
  await user.type(name, 'Shows');
  await user.click(screen.getByRole('switch', { name: 'Enabled' }));
  await user.click(screen.getByRole('button', { name: 'Create library' }));

  expect(name).toHaveValue('Shows');
  expect(screen.getByRole('switch', { name: 'Enabled' })).not.toBeChecked();
  expect(screen.getByRole('dialog', { name: 'Add library' })).toBeVisible();

  view.rerender(
    <LibraryCreateDialog isOpen isPending onClose={onClose} onCreate={onCreate} />,
  );
  expect(screen.getByRole('button', { name: 'Create library' })).toHaveAttribute('data-pending', 'true');
  expect(screen.getByRole('button', { name: 'Create library' })).toHaveAttribute('aria-disabled', 'true');
  expect(screen.getByRole('button', { name: 'Cancel' })).toBeDisabled();
  await user.keyboard('{Escape}');
  expect(screen.getByRole('dialog', { name: 'Add library' })).toBeVisible();
});
