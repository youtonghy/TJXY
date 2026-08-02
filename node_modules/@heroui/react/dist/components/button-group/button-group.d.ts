import type { DOMRenderProps } from "../../utils/dom";
import type { ButtonProps } from "../button";
import type { ButtonGroupVariants } from "@heroui/styles";
import type { ComponentPropsWithRef, ReactNode } from "react";
import { buttonGroupVariants } from "@heroui/styles";
import React from "react";
import { Group } from "react-aria-components/Group";
type ButtonGroupContext = {
    slots?: ReturnType<typeof buttonGroupVariants>;
    size?: ButtonProps["size"];
    variant?: ButtonProps["variant"];
    isDisabled?: ButtonProps["isDisabled"];
    fullWidth?: ButtonProps["fullWidth"];
};
declare const ButtonGroupContext: React.Context<ButtonGroupContext>;
export declare const BUTTON_GROUP_CHILD = "__button_group_child";
interface ButtonGroupRootProps extends ComponentPropsWithRef<typeof Group>, Pick<ButtonProps, "size" | "variant">, ButtonGroupVariants {
    /** The orientation of the button group */
    orientation?: "horizontal" | "vertical";
}
declare const ButtonGroupRoot: ({ children, className, fullWidth, isDisabled, orientation: orientationProp, size, variant, ...rest }: ButtonGroupRootProps) => import("react/jsx-runtime").JSX.Element;
interface ButtonGroupSeparatorProps<E extends keyof React.JSX.IntrinsicElements = "span"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const ButtonGroupSeparator: <E extends keyof React.JSX.IntrinsicElements = "span">({ className, ...props }: ButtonGroupSeparatorProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof ButtonGroupSeparatorProps<E>>) => import("react/jsx-runtime").JSX.Element;
export { ButtonGroupRoot, ButtonGroupSeparator, ButtonGroupContext };
export type { ButtonGroupRootProps, ButtonGroupSeparatorProps };
