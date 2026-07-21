import { Chip, Stack } from '@mui/material';

export function UserStatus({ isAdministrator, isDisabled }: {
  isAdministrator: boolean;
  isDisabled: boolean;
}) {
  return (
    <Stack direction="row" spacing={1} useFlexGap sx={{ flexWrap: 'wrap' }}>
      <Chip size="small" label={isAdministrator ? 'Administrator' : 'User'} color={isAdministrator ? 'primary' : 'default'} />
      <Chip size="small" label={isDisabled ? 'Disabled' : 'Enabled'} color={isDisabled ? 'warning' : 'success'} variant="outlined" />
    </Stack>
  );
}
