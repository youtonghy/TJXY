import { render, screen } from '@testing-library/react';
import { AnnouncementMarkdown } from './AnnouncementMarkdown';

it('renders useful Markdown while dropping active content and unsafe URLs', () => {
  const { container } = render(
    <AnnouncementMarkdown>
      {'## Update\n\n**Ready** [safe](https://example.com) [unsafe](javascript:alert(1))\n\n<img src="https://tracker.example/pixel">'}
    </AnnouncementMarkdown>,
  );

  expect(screen.getByRole('heading', { name: 'Update' })).toBeVisible();
  expect(screen.getByText('Ready')).toBeVisible();
  expect(screen.getByRole('link', { name: 'safe' })).toHaveAttribute('href', 'https://example.com');
  expect(screen.queryByRole('link', { name: 'unsafe' })).not.toBeInTheDocument();
  expect(container.querySelector('img')).toBeNull();
});
