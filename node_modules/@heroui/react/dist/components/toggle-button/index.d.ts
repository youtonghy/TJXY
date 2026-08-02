import type { ComponentProps } from "react";
import { ToggleButtonRoot } from "./toggle-button";
export declare const ToggleButton: (({ children, className, isIconOnly, size, style, variant, ...rest }: import("./toggle-button").ToggleButtonRootProps) => import("react/jsx-runtime").JSX.Element) & {
    Root: ({ children, className, isIconOnly, size, style, variant, ...rest }: import("./toggle-button").ToggleButtonRootProps) => import("react/jsx-runtime").JSX.Element;
};
export type ToggleButton = {
    Props: ComponentProps<typeof ToggleButtonRoot>;
    RootProps: ComponentProps<typeof ToggleButtonRoot>;
};
export { ToggleButtonRoot };
export type { ToggleButtonRootProps, ToggleButtonRootProps as ToggleButtonProps, } from "./toggle-button";
export { toggleButtonVariants } from "@heroui/styles";
export type { ToggleButtonVariants } from "@heroui/styles";
