import { TableCell, Typography } from '@mui/material';

export function ResponsiveTableCell({
  children,
  label,
}: {
  children: React.ReactNode;
  label: string;
}) {
  return (
    <TableCell sx={{ border: 0, display: { xs: 'grid', sm: 'table-cell' }, gap: 0.25, minWidth: 0, overflowWrap: 'anywhere', px: { xs: 0, sm: 2 }, py: 0.75 }}>
      <Typography color="text.secondary" component="span" sx={{ display: { sm: 'none' } }} variant="caption">{label}</Typography>
      {children}
    </TableCell>
  );
}
