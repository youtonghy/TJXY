import { Button } from '@heroui/react';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { ConfirmDialog } from './ConfirmDialog';

function renderDialog(onConfirm: () => void | Promise<void> = vi.fn()) {
  return render(
    <ConfirmDialog
      trigger={<Button>Delete Ada</Button>}
      title="Delete Ada?"
      description="Ada will lose administrator access."
      confirmLabel="Delete user"
      isPending={false}
      onConfirm={onConfirm}
    />,
  );
}

describe('ConfirmDialog', () => {
  it('names the target and initially focuses the least-destructive action', async () => {
    const user = userEvent.setup();
    renderDialog();

    await user.click(screen.getByRole('button', { name: 'Delete Ada' }));

    expect(screen.getByRole('dialog', { name: 'Delete Ada?' })).toBeVisible();
    expect(screen.getByText('Ada will lose administrator access.')).toBeVisible();
    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Cancel' })).toHaveFocus();
    });
  });

  it('locks dismissal and repeated confirmation while pending', async () => {
    const user = userEvent.setup();
    const view = renderDialog();
    await user.click(screen.getByRole('button', { name: 'Delete Ada' }));

    view.rerender(
      <ConfirmDialog
        trigger={<Button>Delete Ada</Button>}
        title="Delete Ada?"
        description="Ada will lose administrator access."
        confirmLabel="Delete user"
        isPending
        onConfirm={vi.fn()}
      />,
    );

    expect(screen.getByRole('button', { name: 'Cancel' })).toBeDisabled();
    const confirm = screen.getByRole('button', { name: 'Delete user' });
    expect(confirm).toBeDisabled();
    expect(within(confirm).getByText('Delete user')).toHaveClass('min-h-5');
    expect(screen.getByRole('button', { name: 'Close' })).toBeDisabled();
    await user.keyboard('{Escape}');
    expect(screen.getByRole('dialog', { name: 'Delete Ada?' })).toBeVisible();

    view.rerender(
      <ConfirmDialog
        trigger={<Button>Delete Ada</Button>}
        title="Delete Ada?"
        description="Ada will lose administrator access."
        confirmLabel="Delete user"
        isPending={false}
        onConfirm={vi.fn()}
      />,
    );
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    await waitFor(() => {
      expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    });
  });

  it('keeps the dialog open with an inline error when confirmation rejects', async () => {
    const user = userEvent.setup();
    renderDialog(vi.fn().mockRejectedValue(new Error('sensitive detail')));
    await user.click(screen.getByRole('button', { name: 'Delete Ada' }));
    await user.click(screen.getByRole('button', { name: 'Delete user' }));

    expect(await screen.findByRole('alert')).toHaveTextContent('could not be completed');
    expect(screen.queryByText('sensitive detail')).not.toBeInTheDocument();
    expect(screen.getByRole('dialog', { name: 'Delete Ada?' })).toBeVisible();
  });

  it('closes after success and restores focus to the trigger', async () => {
    const user = userEvent.setup();
    renderDialog(vi.fn().mockResolvedValue(undefined));
    const trigger = screen.getByRole('button', { name: 'Delete Ada' });
    await user.click(trigger);
    await user.click(screen.getByRole('button', { name: 'Delete user' }));

    await waitFor(() => {
      expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    });
    expect(trigger).toHaveFocus();
  });
});
