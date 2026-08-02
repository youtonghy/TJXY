import type { ComponentProps } from "react";
import { ButtonGroupRoot, ButtonGroupSeparator } from "./button-group";
export declare const ButtonGroup: (({ children, className, fullWidth, isDisabled, orientation: orientationProp, size, variant, ...rest }: import("./button-group").ButtonGroupRootProps) => import("react/jsx-runtime").JSX.Element) & {
    Root: ({ children, className, fullWidth, isDisabled, orientation: orientationProp, size, variant, ...rest }: import("./button-group").ButtonGroupRootProps) => import("react/jsx-runtime").JSX.Element;
    Separator: <E extends keyof React.JSX.IntrinsicElements = "span">({ className, ...props }: import("./button-group").ButtonGroupSeparatorProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./button-group").ButtonGroupSeparatorProps<E>>) => import("react/jsx-runtime").JSX.Element;
};
export type ButtonGroup = {
    Props: ComponentProps<typeof ButtonGroupRoot>;
    RootProps: ComponentProps<typeof ButtonGroupRoot>;
    SeparatorProps: ComponentProps<typeof ButtonGroupSeparator>;
};
export { ButtonGroupRoot, ButtonGroupSeparator };
export type { ButtonGroupRootProps, ButtonGroupRootProps as ButtonGroupProps, ButtonGroupSeparatorProps, } from "./button-group";
export { ButtonGroupContext, BUTTON_GROUP_CHILD } from "./button-group";
export { buttonGroupVariants } from "@heroui/styles";
export type { ButtonGroupVariants } from "@heroui/styles";
