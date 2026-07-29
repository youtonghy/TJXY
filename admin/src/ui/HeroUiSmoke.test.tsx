import { Button, Input, Label, TextField } from '@heroui/react';
import { render, screen } from '@testing-library/react';

describe('HeroUI foundation', () => {
  it('renders accessible HeroUI controls', () => {
    render(
      <>
        <TextField>
          <Label>Administrator name</Label>
          <Input />
        </TextField>
        <Button>Save changes</Button>
      </>,
    );

    expect(screen.getByRole('textbox', { name: 'Administrator name' })).toBeVisible();
    expect(screen.getByRole('button', { name: 'Save changes' })).toBeEnabled();
  });
});
