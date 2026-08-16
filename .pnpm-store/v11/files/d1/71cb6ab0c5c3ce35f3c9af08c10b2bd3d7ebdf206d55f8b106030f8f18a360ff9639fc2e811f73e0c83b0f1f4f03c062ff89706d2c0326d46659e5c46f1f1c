import type { DOMRenderProps } from "../../utils/dom";
import type { ComponentPropsWithRef, ReactNode } from "react";
import type { RadioButtonRenderProps, RadioFieldRenderProps } from "react-aria-components/RadioGroup";
import React from "react";
import { RadioButton as RadioButtonPrimitive, RadioField as RadioFieldPrimitive } from "react-aria-components/RadioGroup";
interface RadioRootProps extends ComponentPropsWithRef<typeof RadioFieldPrimitive> {
    /** The name of the radio button, used when submitting an HTML form. */
    name?: string;
}
declare const RadioRoot: {
    ({ children, className, ...props }: RadioRootProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface RadioContentProps extends ComponentPropsWithRef<typeof RadioButtonPrimitive> {
}
declare const RadioContent: {
    ({ children, className, ...props }: RadioContentProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface RadioControlProps<E extends keyof React.JSX.IntrinsicElements = "span"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const RadioControl: {
    <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...props }: RadioControlProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof RadioControlProps<E>>): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface RadioIndicatorProps<E extends keyof React.JSX.IntrinsicElements = "span"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode | ((props: RadioFieldRenderProps) => ReactNode);
    className?: string;
}
declare const RadioIndicator: {
    <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...props }: RadioIndicatorProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof RadioIndicatorProps<E>>): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
export { RadioRoot, RadioContent, RadioControl, RadioIndicator };
export type { RadioRootProps, RadioContentProps, RadioControlProps, RadioIndicatorProps, RadioFieldRenderProps, RadioButtonRenderProps, };
