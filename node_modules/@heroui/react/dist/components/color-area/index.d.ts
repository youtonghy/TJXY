import type { ComponentProps } from "react";
import { ColorAreaRoot, ColorAreaThumb } from "./color-area";
export declare const ColorArea: (({ children, className, showDots, style, ...props }: import("./color-area").ColorAreaRootProps) => import("react/jsx-runtime").JSX.Element) & {
    Root: ({ children, className, showDots, style, ...props }: import("./color-area").ColorAreaRootProps) => import("react/jsx-runtime").JSX.Element;
    Thumb: ({ className, style, ...props }: import("./color-area").ColorAreaThumbProps) => import("react/jsx-runtime").JSX.Element;
};
export type ColorArea = {
    Props: ComponentProps<typeof ColorAreaRoot>;
    RootProps: ComponentProps<typeof ColorAreaRoot>;
    ThumbProps: ComponentProps<typeof ColorAreaThumb>;
};
export { ColorAreaRoot, ColorAreaThumb };
export type { ColorAreaRootProps, ColorAreaRootProps as ColorAreaProps, ColorAreaThumbProps, } from "./color-area";
export { colorAreaVariants } from "@heroui/styles";
export type { ColorAreaVariants } from "@heroui/styles";
