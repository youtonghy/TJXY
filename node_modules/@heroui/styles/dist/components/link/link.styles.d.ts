import type { VariantProps } from "tailwind-variants";
export declare const linkVariants: import("tailwind-variants").TVReturnType<{
    [key: string]: {
        [key: string]: import("tailwind-variants").ClassValue | {
            base?: import("tailwind-variants").ClassValue;
            icon?: import("tailwind-variants").ClassValue;
        };
    };
} | {
    [x: string]: {
        [x: string]: import("tailwind-variants").ClassValue | {
            base?: import("tailwind-variants").ClassValue;
            icon?: import("tailwind-variants").ClassValue;
        };
    };
} | {}, {
    base: string;
    icon: string;
}, undefined, {
    [key: string]: {
        [key: string]: import("tailwind-variants").ClassValue | {
            base?: import("tailwind-variants").ClassValue;
            icon?: import("tailwind-variants").ClassValue;
        };
    };
} | {}, {
    base: string;
    icon: string;
}, import("tailwind-variants").TVReturnTypeLike<unknown, {
    base: string;
    icon: string;
}>>;
type LinkRenderPropsKeys = "isCurrent" | "isHovered" | "isPressed" | "isFocused" | "isFocusVisible" | "isDisabled";
export type LinkVariants = Omit<VariantProps<typeof linkVariants>, LinkRenderPropsKeys>;
export {};
//# sourceMappingURL=link.styles.d.ts.map