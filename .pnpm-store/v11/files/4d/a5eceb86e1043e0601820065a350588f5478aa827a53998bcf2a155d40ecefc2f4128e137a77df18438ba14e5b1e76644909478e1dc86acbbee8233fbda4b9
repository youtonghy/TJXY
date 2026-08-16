export declare const mapPropsVariants: <T extends Record<string, any>, K extends keyof T>(props: T, variantKeys?: K[], removeVariantProps?: boolean) => readonly [Omit<T, K> | T, Pick<T, K> | {}];
/**
 * Utility for creating BEM-style class names with variants
 */
export type VariantConfig<T extends Record<string, any>> = {
    base: string;
    variants?: T;
    modifiers?: Record<string, boolean | undefined>;
};
/**
 * Creates a class name builder for components with variants
 * @param config - The variant configuration
 * @returns A function that builds the class name string
 *
 * @example
 * const getButtonClasses = createVariantBuilder({
 *   base: 'button',
 *   variants: {
 *     size: 'md',
 *     variant: 'primary'
 *   },
 *   modifiers: {
 *     'icon-only': isIconOnly
 *   }
 * });
 * // Returns: "button button--md button--primary button--icon-only"
 */
export declare function createVariantBuilder<T extends Record<string, string>>(baseClass: string): (config?: {
    variants?: Partial<T>;
    modifiers?: Record<string, boolean | undefined>;
}) => string;
/**
 * Alternative API using a more declarative approach
 */
export interface VariantDefinition<V extends Record<string, readonly string[]>> {
    base: string;
    variants: V;
    defaults?: Partial<{
        [K in keyof V]: V[K][number];
    }>;
}
/**
 * Creates a typed variant class builder
 * @param definition - The variant definition
 * @returns A function that builds class names based on variant props
 *
 * @example
 * const buttonVariants = createVariants({
 *   base: 'button',
 *   variants: {
 *     size: ['sm', 'md', 'lg'] as const,
 *     variant: ['primary', 'secondary', 'tertiary', 'ghost', 'danger'] as const,
 *   },
 *   defaults: {
 *     size: 'md',
 *     variant: 'primary'
 *   }
 * });
 *
 * const className = buttonVariants({
 *   size: 'lg',
 *   variant: 'danger',
 *   modifiers: { 'icon-only': true }
 * });
 * // Returns: "button button--lg button--danger button--icon-only"
 */
export declare function createVariants<V extends Record<string, readonly string[]>>(definition: VariantDefinition<V>): (props?: { [K in keyof V]?: V[K][number] | undefined; } & {
    modifiers?: Record<string, boolean | undefined>;
}) => string;
/**
 * Type helpers for extracting variant props from a variant definition
 */
export type VariantPropsOf<T extends ReturnType<typeof createVariants>> = T extends (props: infer P) => string ? P : never;
