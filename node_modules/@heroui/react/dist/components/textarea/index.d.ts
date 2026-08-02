import type { ComponentProps } from "react";
import { TextAreaRoot } from "./textarea";
export declare const TextArea: (({ className, fullWidth, variant, ...rest }: import("./textarea").TextAreaRootProps) => import("react/jsx-runtime").JSX.Element) & {
    Root: ({ className, fullWidth, variant, ...rest }: import("./textarea").TextAreaRootProps) => import("react/jsx-runtime").JSX.Element;
};
export type TextArea = {
    Props: ComponentProps<typeof TextAreaRoot>;
    RootProps: ComponentProps<typeof TextAreaRoot>;
};
export { TextAreaRoot };
export type { TextAreaRootProps, TextAreaRootProps as TextAreaProps } from "./textarea";
export { textAreaVariants } from "@heroui/styles";
export type { TextAreaVariants } from "@heroui/styles";
