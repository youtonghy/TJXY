import type { ComponentProps } from "react";
import { ColorInputGroupInput, ColorInputGroupPrefix, ColorInputGroupRoot, ColorInputGroupSuffix } from "../color-input-group";
import { ColorFieldRoot } from "./color-field";
export declare const ColorField: typeof ColorFieldRoot & {
    Root: typeof ColorFieldRoot;
    Group: ({ children, className, fullWidth, variant, ...props }: import("../color-input-group").ColorInputGroupRootProps) => import("react/jsx-runtime").JSX.Element;
    Input: ({ className, ...props }: import("../color-input-group").ColorInputGroupInputProps) => import("react/jsx-runtime").JSX.Element;
    Prefix: <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...props }: import("../color-input-group").ColorInputGroupPrefixProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("../color-input-group").ColorInputGroupPrefixProps<E>>) => import("react/jsx-runtime").JSX.Element;
    Suffix: <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...props }: import("../color-input-group").ColorInputGroupSuffixProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("../color-input-group").ColorInputGroupSuffixProps<E>>) => import("react/jsx-runtime").JSX.Element;
};
export type ColorField = {
    Props: ComponentProps<typeof ColorFieldRoot>;
    RootProps: ComponentProps<typeof ColorFieldRoot>;
    GroupProps: ComponentProps<typeof ColorInputGroupRoot>;
    InputProps: ComponentProps<typeof ColorInputGroupInput>;
    PrefixProps: ComponentProps<typeof ColorInputGroupPrefix>;
    SuffixProps: ComponentProps<typeof ColorInputGroupSuffix>;
};
export { ColorFieldRoot };
export type { ColorFieldRootProps, ColorFieldRootProps as ColorFieldProps, ColorValue, } from "./color-field";
export { colorFieldVariants } from "@heroui/styles";
export type { ColorFieldVariants } from "@heroui/styles";
