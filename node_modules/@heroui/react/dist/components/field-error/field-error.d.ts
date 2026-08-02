import type { FieldErrorVariants } from "@heroui/styles";
import type { ComponentPropsWithRef } from "react";
import { FieldError as FieldErrorPrimitive } from "react-aria-components/FieldError";
interface FieldErrorRootProps extends ComponentPropsWithRef<typeof FieldErrorPrimitive>, FieldErrorVariants {
}
declare const FieldErrorRoot: ({ children, className, ...rest }: FieldErrorRootProps) => import("react/jsx-runtime").JSX.Element;
export { FieldErrorRoot };
export type { FieldErrorRootProps };
