import type { TextFieldVariants } from "@heroui/styles";
import type { ComponentPropsWithRef } from "react";
import React from "react";
import { TextField as TextFieldPrimitive } from "react-aria-components/TextField";
type TextFieldContext = {
    variant?: "primary" | "secondary";
};
declare const TextFieldContext: React.Context<TextFieldContext>;
interface TextFieldRootProps extends ComponentPropsWithRef<typeof TextFieldPrimitive>, TextFieldVariants {
    /**
     * The variant of the text field.
     * @default "primary"
     */
    variant?: "primary" | "secondary";
}
declare const TextFieldRoot: ({ children, className, fullWidth, variant, ...props }: TextFieldRootProps) => import("react/jsx-runtime").JSX.Element;
export { TextFieldRoot, TextFieldContext };
export type { TextFieldRootProps };
