import type { ComponentProps } from "react";
import { ScrollShadowRoot } from "./scroll-shadow";
export declare const ScrollShadow: {
    ({ children, className, hideScrollBar, isEnabled, offset, onVisibilityChange, orientation, ref, size, variant, visibility, ...props }: import("./scroll-shadow").ScrollShadowRootProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
} & {
    Root: {
        ({ children, className, hideScrollBar, isEnabled, offset, onVisibilityChange, orientation, ref, size, variant, visibility, ...props }: import("./scroll-shadow").ScrollShadowRootProps): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
};
export type ScrollShadow = {
    Props: ComponentProps<typeof ScrollShadowRoot>;
    RootProps: ComponentProps<typeof ScrollShadowRoot>;
};
export { ScrollShadowRoot };
export type { ScrollShadowRootProps, ScrollShadowRootProps as ScrollShadowProps, ScrollShadowVisibility, } from "./scroll-shadow";
export { scrollShadowVariants } from "@heroui/styles";
export type { ScrollShadowVariants } from "@heroui/styles";
export { useScrollShadow } from "./use-scroll-shadow";
export type { UseScrollShadowProps } from "./use-scroll-shadow";
