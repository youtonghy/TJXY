import type { ScrollShadowVariants } from "@heroui/styles";
export type ScrollShadowVisibility = "auto" | "both" | "top" | "bottom" | "left" | "right" | "none";
export interface ScrollShadowRootProps extends Omit<React.ComponentProps<"div">, "size">, ScrollShadowVariants {
    /**
     * The shadow size in pixels
     * @default 40
     */
    size?: number;
    /**
     * The scroll offset before showing shadows (in pixels)
     * @default 0
     */
    offset?: number;
    /**
     * Controlled shadow visibility state
     * @default "auto"
     */
    visibility?: ScrollShadowVisibility;
    /**
     * Whether scroll shadow detection is enabled
     * @default true
     */
    isEnabled?: boolean;
    /**
     * Callback invoked when shadow visibility changes
     */
    onVisibilityChange?: (visibility: ScrollShadowVisibility) => void;
}
export declare const ScrollShadowRoot: {
    ({ children, className, hideScrollBar, isEnabled, offset, onVisibilityChange, orientation, ref, size, variant, visibility, ...props }: ScrollShadowRootProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
