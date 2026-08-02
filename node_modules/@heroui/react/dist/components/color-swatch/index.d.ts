import type { ComponentProps } from "react";
import { ColorSwatchRoot } from "./color-swatch";
export declare const ColorSwatch: (({ className, shape, size, style, ...props }: import("./color-swatch").ColorSwatchRootProps) => import("react/jsx-runtime").JSX.Element) & {
    Root: ({ className, shape, size, style, ...props }: import("./color-swatch").ColorSwatchRootProps) => import("react/jsx-runtime").JSX.Element;
};
export type ColorSwatch = {
    Props: ComponentProps<typeof ColorSwatchRoot>;
    RootProps: ComponentProps<typeof ColorSwatchRoot>;
};
export { ColorSwatchRoot };
export type { ColorSwatchRootProps, ColorSwatchRootProps as ColorSwatchProps } from "./color-swatch";
export { colorSwatchVariants } from "@heroui/styles";
export type { ColorSwatchVariants } from "@heroui/styles";
