import type { CheckboxVariants } from "../checkbox";
import type { CheckboxGroupVariants } from "@heroui/styles";
import type { ComponentPropsWithRef } from "react";
import React from "react";
import { CheckboxGroup as CheckboxGroupPrimitive } from "react-aria-components/CheckboxGroup";
interface CheckboxGroupContext {
    variant?: CheckboxVariants["variant"];
}
declare const CheckboxGroupContext: React.Context<CheckboxGroupContext>;
interface CheckboxGroupProps extends ComponentPropsWithRef<typeof CheckboxGroupPrimitive>, CheckboxGroupVariants {
}
declare const CheckboxGroup: ({ children, className, variant, ...props }: CheckboxGroupProps) => import("react/jsx-runtime").JSX.Element;
export { CheckboxGroup, CheckboxGroupContext };
export type { CheckboxGroupProps };
