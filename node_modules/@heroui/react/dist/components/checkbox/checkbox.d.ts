import type { DOMRenderProps } from "../../utils/dom";
import type { CheckboxVariants } from "@heroui/styles";
import type { ComponentPropsWithRef, ReactNode } from "react";
import type { CheckboxButtonRenderProps, CheckboxFieldRenderProps } from "react-aria-components/Checkbox";
import React from "react";
import { CheckboxButton as CheckboxButtonPrimitive, CheckboxField as CheckboxFieldPrimitive } from "react-aria-components/Checkbox";
interface CheckboxRootProps extends ComponentPropsWithRef<typeof CheckboxFieldPrimitive>, CheckboxVariants {
    /** The name of the checkbox, used when submitting an HTML form. */
    name?: string;
}
declare const CheckboxRoot: {
    ({ children, className, variant, ...props }: CheckboxRootProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface CheckboxContentProps extends ComponentPropsWithRef<typeof CheckboxButtonPrimitive> {
}
declare const CheckboxContent: {
    ({ children, className, ...props }: CheckboxContentProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface CheckboxControlProps<E extends keyof React.JSX.IntrinsicElements = "span"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const CheckboxControl: {
    <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...props }: CheckboxControlProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof CheckboxControlProps<E>>): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface CheckboxIndicatorProps<E extends keyof React.JSX.IntrinsicElements = "span"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode | ((props: CheckboxFieldRenderProps) => ReactNode);
    className?: string;
}
declare const CheckboxIndicator: {
    <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...props }: CheckboxIndicatorProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof CheckboxIndicatorProps<E>>): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
export { CheckboxRoot, CheckboxContent, CheckboxControl, CheckboxIndicator };
export type { CheckboxRootProps, CheckboxContentProps, CheckboxControlProps, CheckboxIndicatorProps, CheckboxFieldRenderProps, CheckboxButtonRenderProps, };
