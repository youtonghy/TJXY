import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { SystemLocaleProvider } from '../../settings/SystemLocaleProvider';
import { ServerAddressField } from './ServerAddressField';

it('keeps the server address hidden until a custom server is added', async () => {
  const user = userEvent.setup();
  let value = 'https://tjxy.200461.xyz';
  const onChange = vi.fn((next: string) => { value = next; });
  const { rerender } = render(
    <SystemLocaleProvider>
      <ServerAddressField value={value} onChange={onChange} onSave={vi.fn()} />
    </SystemLocaleProvider>,
  );

  expect(screen.queryByRole('textbox', { name: /Server address/ })).not.toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: /Add server|添加服务器/ }));
  expect(screen.getByRole('textbox', { name: /Server address|服务器地址/ })).toBeVisible();
  await user.click(screen.getByRole('button', { name: /Cancel|取消/ }));
  rerender(
    <SystemLocaleProvider>
      <ServerAddressField value={value} onChange={onChange} onSave={vi.fn()} />
    </SystemLocaleProvider>,
  );
  expect(screen.queryByRole('textbox', { name: /Server address/ })).not.toBeInTheDocument();
});
