import type { ComponentProps } from "react";
import { FieldGroup, FieldsetActions, FieldsetLegend, FieldsetRoot } from "./fieldset";
export declare const Fieldset: (<E extends keyof React.JSX.IntrinsicElements = "fieldset">({ children, className, ...props }: import("./fieldset").FieldsetRootProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./fieldset").FieldsetRootProps<E>>) => import("react/jsx-runtime").JSX.Element) & {
    Root: <E extends keyof React.JSX.IntrinsicElements = "fieldset">({ children, className, ...props }: import("./fieldset").FieldsetRootProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./fieldset").FieldsetRootProps<E>>) => import("react/jsx-runtime").JSX.Element;
    Legend: <E extends keyof React.JSX.IntrinsicElements = "legend">({ className, ...props }: import("./fieldset").FieldsetLegendProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./fieldset").FieldsetLegendProps<E>>) => import("react/jsx-runtime").JSX.Element;
    Group: <E extends keyof React.JSX.IntrinsicElements = "div">({ className, ...rest }: import("./fieldset").FieldGroupProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./fieldset").FieldGroupProps<E>>) => import("react/jsx-runtime").JSX.Element;
    Actions: <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...rest }: import("./fieldset").FieldsetActionsProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./fieldset").FieldsetActionsProps<E>>) => import("react/jsx-runtime").JSX.Element;
};
export type Fieldset = {
    Props: ComponentProps<typeof FieldsetRoot>;
    RootProps: ComponentProps<typeof FieldsetRoot>;
    LegendProps: ComponentProps<typeof FieldsetLegend>;
    GroupProps: ComponentProps<typeof FieldGroup>;
    ActionsProps: ComponentProps<typeof FieldsetActions>;
};
export { FieldsetRoot, FieldsetLegend, FieldGroup, FieldsetActions };
export type { FieldsetRootProps, FieldsetRootProps as FieldsetProps, FieldsetLegendProps, FieldGroupProps, FieldsetActionsProps, } from "./fieldset";
export { fieldsetVariants } from "@heroui/styles";
export type { FieldsetVariants } from "@heroui/styles";
