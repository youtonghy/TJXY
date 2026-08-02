import type { ComponentProps } from "react";
import { TagRemoveButton, TagRoot } from "./tag";
export declare const Tag: (({ children, className, ...restProps }: import("./tag").TagRootProps) => import("react/jsx-runtime").JSX.Element) & {
    Root: ({ children, className, ...restProps }: import("./tag").TagRootProps) => import("react/jsx-runtime").JSX.Element;
    RemoveButton: ({ children, className, ...restProps }: import("./tag").TagRemoveButtonProps) => import("react/jsx-runtime").JSX.Element;
};
export type Tag = {
    Props: ComponentProps<typeof TagRoot>;
    RootProps: ComponentProps<typeof TagRoot>;
    RemoveButtonProps: ComponentProps<typeof TagRemoveButton>;
};
export { TagRoot, TagRemoveButton };
export type { TagRootProps, TagRemoveButtonProps } from "./tag";
export { tagVariants } from "@heroui/styles";
export type { TagVariants } from "@heroui/styles";
