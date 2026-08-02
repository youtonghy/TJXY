import type { ComponentProps } from "react";
import { TextFieldRoot } from "./textfield";
export declare const TextField: (({ children, className, fullWidth, variant, ...props }: import("./textfield").TextFieldRootProps) => import("react/jsx-runtime").JSX.Element) & {
    Root: ({ children, className, fullWidth, variant, ...props }: import("./textfield").TextFieldRootProps) => import("react/jsx-runtime").JSX.Element;
};
export type TextField = {
    Props: ComponentProps<typeof TextFieldRoot>;
    RootProps: ComponentProps<typeof TextFieldRoot>;
};
export { TextFieldRoot };
export type { TextFieldRootProps, TextFieldRootProps as TextFieldProps } from "./textfield";
export { TextFieldContext } from "./textfield";
export { textFieldVariants } from "@heroui/styles";
export type { TextFieldVariants } from "@heroui/styles";
