export type Theme = string;
/**
 * React hook to switch between themes.
 *
 * Accepts any theme name ("light", "dark", "brutalism-light", etc.).
 * Pass "system" to follow the OS preference (resolves to "light" or "dark").
 *
 * @param defaultTheme - the initial theme name (defaults to "system")
 * @returns `theme` (the stored intent, e.g. "system"), `resolvedTheme` (the
 *          concrete class applied to the DOM — undefined during SSR), and `setTheme`.
 */
export declare function useTheme(defaultTheme?: Theme): {
    resolvedTheme: string | undefined;
    setTheme: (newTheme: Theme) => void;
    theme: string;
};
