import { Button } from '@heroui/react';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { StorageWorkflow } from './StorageWorkflow';

it('renders non-clickable phase progress and preserves its child content', async () => {
  const user = userEvent.setup();
  const restart = vi.fn();
  const { rerender } = render(
    <StorageWorkflow
      canRestart={false}
      isBusy={false}
      onRestart={restart}
      phase="authorize"
      providerName="Google Drive"
      title="Google Drive"
    >
      <p>Provider content</p>
    </StorageWorkflow>,
  );

  const progress = screen.getByRole('list', { name: 'Google Drive setup progress' });
  expect(progress).toHaveTextContent('Authorize');
  expect(progress).toHaveTextContent('Choose folder');
  expect(progress).toHaveTextContent('Review');
  expect(screen.getByRole('listitem', { current: 'step' })).toHaveTextContent('Authorize');
  expect(screen.queryByRole('button', { name: 'Restart authorization' })).not.toBeInTheDocument();
  expect(screen.getByText('Provider content')).toBeVisible();

  rerender(
    <StorageWorkflow
      canRestart
      isBusy={false}
      onRestart={restart}
      phase="choose-folder"
      providerName="Google Drive"
      title="Google Drive"
    >
      <p>Provider content</p>
    </StorageWorkflow>,
  );
  const restartButton = screen.getByRole('button', { name: 'Restart authorization' });
  expect(screen.getByRole('listitem', { current: 'step' })).toHaveTextContent('Choose folder');
  await user.click(restartButton);
  expect(restart).toHaveBeenCalledOnce();
});

it('prevents restart while a provider command is pending and marks complete progress', () => {
  render(
    <StorageWorkflow
      canRestart
      isBusy
      onRestart={vi.fn()}
      phase="complete"
      providerName="OneDrive"
      title="OneDrive"
    >
      <Button>Child action</Button>
    </StorageWorkflow>,
  );

  expect(screen.getByRole('button', { name: 'Restart authorization' })).toBeDisabled();
  expect(screen.queryByRole('listitem', { current: 'step' })).not.toBeInTheDocument();
  expect(screen.getAllByText('Complete')).toHaveLength(3);
  expect(screen.getByRole('button', { name: 'Child action' })).toBeVisible();
});
