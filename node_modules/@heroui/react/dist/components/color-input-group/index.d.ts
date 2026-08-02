import type { ComponentProps } from "react";
import { ColorInputGroupInput, ColorInputGroupPrefix, ColorInputGroupRoot, ColorInputGroupSuffix } from "./color-input-group";
export declare const ColorInputGroup: (({ children, className, fullWidth, variant, ...props }: import("./color-input-group").ColorInputGroupRootProps) => import("react/jsx-runtime").JSX.Element) & {
    Root: ({ children, className, fullWidth, variant, ...props }: import("./color-input-group").ColorInputGroupRootProps) => import("react/jsx-runtime").JSX.Element;
    Input: ({ className, ...props }: import("./color-input-group").ColorInputGroupInputProps) => import("react/jsx-runtime").JSX.Element;
    Prefix: <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...props }: import("./color-input-group").ColorInputGroupPrefixProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./color-input-group").ColorInputGroupPrefixProps<E>>) => import("react/jsx-runtime").JSX.Element;
    Suffix: <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...props }: import("./color-input-group").ColorInputGroupSuffixProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./color-input-group").ColorInputGroupSuffixProps<E>>) => import("react/jsx-runtime").JSX.Element;
};
export type ColorInputGroup = {
    Props: ComponentProps<typeof ColorInputGroupRoot>;
    RootProps: ComponentProps<typeof ColorInputGroupRoot>;
    InputProps: ComponentProps<typeof ColorInputGroupInput>;
    PrefixProps: ComponentProps<typeof ColorInputGroupPrefix>;
    SuffixProps: ComponentProps<typeof ColorInputGroupSuffix>;
};
export { ColorInputGroupInput, ColorInputGroupPrefix, ColorInputGroupRoot, ColorInputGroupSuffix };
export type { ColorInputGroupRootProps, ColorInputGroupRootProps as ColorInputGroupProps, ColorInputGroupInputProps, ColorInputGroupPrefixProps, ColorInputGroupSuffixProps, } from "./color-input-group";
export { colorInputGroupVariants } from "@heroui/styles";
export type { ColorInputGroupVariants } from "@heroui/styles";
