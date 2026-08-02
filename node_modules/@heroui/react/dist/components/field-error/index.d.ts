import type { ComponentProps } from "react";
import { FieldErrorRoot } from "./field-error";
export declare const FieldError: (({ children, className, ...rest }: import("./field-error").FieldErrorRootProps) => import("react/jsx-runtime").JSX.Element) & {
    Root: ({ children, className, ...rest }: import("./field-error").FieldErrorRootProps) => import("react/jsx-runtime").JSX.Element;
};
export type FieldError = {
    Props: ComponentProps<typeof FieldErrorRoot>;
    RootProps: ComponentProps<typeof FieldErrorRoot>;
};
export { FieldErrorRoot };
export type { FieldErrorRootProps, FieldErrorRootProps as FieldErrorProps } from "./field-error";
export { fieldErrorVariants } from "@heroui/styles";
export type { FieldErrorVariants } from "@heroui/styles";
