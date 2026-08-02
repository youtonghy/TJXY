import type { ComponentProps } from "react";
import { ToggleButtonGroupRoot, ToggleButtonGroupSeparator } from "./toggle-button-group";
export declare const ToggleButtonGroup: (({ children, className, fullWidth, isDetached, isDisabled, orientation: orientationProp, size, ...rest }: import("./toggle-button-group").ToggleButtonGroupRootProps) => import("react/jsx-runtime").JSX.Element) & {
    Root: ({ children, className, fullWidth, isDetached, isDisabled, orientation: orientationProp, size, ...rest }: import("./toggle-button-group").ToggleButtonGroupRootProps) => import("react/jsx-runtime").JSX.Element;
    Separator: <E extends keyof React.JSX.IntrinsicElements = "span">({ className, ...props }: import("./toggle-button-group").ToggleButtonGroupSeparatorProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./toggle-button-group").ToggleButtonGroupSeparatorProps<E>>) => import("react/jsx-runtime").JSX.Element;
};
export type ToggleButtonGroup = {
    Props: ComponentProps<typeof ToggleButtonGroupRoot>;
    RootProps: ComponentProps<typeof ToggleButtonGroupRoot>;
    SeparatorProps: ComponentProps<typeof ToggleButtonGroupSeparator>;
};
export { ToggleButtonGroupRoot, ToggleButtonGroupSeparator };
export type { ToggleButtonGroupRootProps, ToggleButtonGroupRootProps as ToggleButtonGroupProps, ToggleButtonGroupSeparatorProps, } from "./toggle-button-group";
export { ToggleButtonGroupContext, TOGGLE_BUTTON_GROUP_CHILD } from "./toggle-button-group";
export { toggleButtonGroupVariants } from "@heroui/styles";
export type { ToggleButtonGroupVariants } from "@heroui/styles";
