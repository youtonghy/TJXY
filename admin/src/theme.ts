import { createTheme } from '@mui/material/styles';

export const theme = createTheme({
  palette: {
    mode: 'light',
    primary: { main: '#087f75', dark: '#065f58', contrastText: '#ffffff' },
    secondary: { main: '#3f5964' },
    background: { default: '#f6f7f8', paper: '#ffffff' },
    text: { primary: '#172126', secondary: '#526169' },
    warning: { main: '#b45309' },
  },
  shape: { borderRadius: 6 },
  typography: {
    fontFamily: 'Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
    h1: { fontSize: '1.75rem', fontWeight: 700, letterSpacing: 0 },
    h2: { fontSize: '1.25rem', fontWeight: 700, letterSpacing: 0 },
    h3: { fontSize: '1rem', fontWeight: 700, letterSpacing: 0 },
    button: { textTransform: 'none', fontWeight: 650, letterSpacing: 0 },
    body1: { letterSpacing: 0 },
    body2: { letterSpacing: 0 },
  },
  components: {
    MuiButton: { styleOverrides: { root: { minHeight: 40, borderRadius: 6 } } },
    MuiIconButton: { styleOverrides: { root: { width: 40, height: 40 } } },
    MuiCard: { styleOverrides: { root: { borderRadius: 8, boxShadow: '0 8px 28px rgba(23,33,38,0.10)' } } },
    MuiPaper: { styleOverrides: { root: { backgroundImage: 'none' } } },
  },
});
