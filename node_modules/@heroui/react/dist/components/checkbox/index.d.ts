import type { ComponentProps } from "react";
import { CheckboxContent, CheckboxControl, CheckboxIndicator, CheckboxRoot } from "./checkbox";
export declare const Checkbox: {
    ({ children, className, variant, ...props }: import("./checkbox").CheckboxRootProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
} & {
    Root: {
        ({ children, className, variant, ...props }: import("./checkbox").CheckboxRootProps): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Content: {
        ({ children, className, ...props }: import("./checkbox").CheckboxContentProps): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Control: {
        <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...props }: import("./checkbox").CheckboxControlProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./checkbox").CheckboxControlProps<E>>): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Indicator: {
        <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...props }: import("./checkbox").CheckboxIndicatorProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./checkbox").CheckboxIndicatorProps<E>>): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
};
export type Checkbox = {
    Props: ComponentProps<typeof CheckboxRoot>;
    RootProps: ComponentProps<typeof CheckboxRoot>;
    ContentProps: ComponentProps<typeof CheckboxContent>;
    ControlProps: ComponentProps<typeof CheckboxControl>;
    IndicatorProps: ComponentProps<typeof CheckboxIndicator>;
};
export { CheckboxContent, CheckboxControl, CheckboxIndicator, CheckboxRoot };
export type { CheckboxRootProps, CheckboxRootProps as CheckboxProps, CheckboxContentProps, CheckboxControlProps, CheckboxIndicatorProps, CheckboxFieldRenderProps, CheckboxButtonRenderProps, 
/** @deprecated Use {@link CheckboxFieldRenderProps} for the root render prop, or {@link CheckboxButtonRenderProps} for content/control/indicator. */
CheckboxFieldRenderProps as CheckboxRenderProps, } from "./checkbox";
export { checkboxVariants } from "@heroui/styles";
export type { CheckboxVariants } from "@heroui/styles";
