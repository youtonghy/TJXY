/**
 * A hook that safely retrieves CSS custom property values from the document element
 * with SSR support, automatic fallback handling, and optional caching.
 *
 * @example
 * ```jsx
 * function Component() {
 *   const animationType = useCSSVariable('--skeleton-animation')
 *   // Returns the CSS variable value or undefined (cached by default)
 * }
 * ```
 *
 * @example
 * ```jsx
 * function Component({ color }) {
 *   // Override with prop if provided
 *   const themeColor = useCSSVariable('--theme-color', color)
 *   // Returns color prop if defined, otherwise CSS variable value
 * }
 * ```
 *
 * @example
 * ```jsx
 * function Component() {
 *   // Disable caching for dynamic CSS variables
 *   const dynamicValue = useCSSVariable('--dynamic-value', undefined, false)
 *   // Will always query the DOM for fresh value
 * }
 * ```
 *
 * @param variableName - The CSS custom property name (e.g., '--my-variable')
 * @param override - Optional override value that takes precedence over CSS variable
 * @param cache - Whether to cache the CSS variable value (default: true)
 * @returns The CSS variable value, override value, or undefined
 */
export declare function useCSSVariable(variableName: string, override?: string, cache?: boolean): string | undefined;
