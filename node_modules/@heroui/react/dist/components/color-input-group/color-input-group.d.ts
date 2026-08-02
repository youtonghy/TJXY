import type { DOMRenderProps } from "../../utils/dom";
import type { ColorInputGroupVariants } from "@heroui/styles";
import type { ComponentPropsWithRef, ReactNode } from "react";
import type { InputProps as InputPrimitiveProps } from "react-aria-components/Input";
import React from "react";
import { Group as GroupPrimitive } from "react-aria-components/Group";
interface ColorInputGroupRootProps extends ComponentPropsWithRef<typeof GroupPrimitive>, ColorInputGroupVariants {
}
declare const ColorInputGroupRoot: ({ children, className, fullWidth, variant, ...props }: ColorInputGroupRootProps) => import("react/jsx-runtime").JSX.Element;
interface ColorInputGroupPrefixProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const ColorInputGroupPrefix: <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...props }: ColorInputGroupPrefixProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof ColorInputGroupPrefixProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface ColorInputGroupInputProps extends InputPrimitiveProps {
    className?: string;
}
declare const ColorInputGroupInput: ({ className, ...props }: ColorInputGroupInputProps) => import("react/jsx-runtime").JSX.Element;
interface ColorInputGroupSuffixProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const ColorInputGroupSuffix: <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...props }: ColorInputGroupSuffixProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof ColorInputGroupSuffixProps<E>>) => import("react/jsx-runtime").JSX.Element;
export { ColorInputGroupRoot, ColorInputGroupInput, ColorInputGroupPrefix, ColorInputGroupSuffix };
export type { ColorInputGroupRootProps, ColorInputGroupInputProps, ColorInputGroupPrefixProps, ColorInputGroupSuffixProps, };
