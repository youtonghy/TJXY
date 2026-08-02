import type { ComponentProps } from "react";
import { SearchFieldClearButton, SearchFieldGroup, SearchFieldInput, SearchFieldRoot, SearchFieldSearchIcon } from "./search-field";
export declare const SearchField: (({ children, className, fullWidth, variant, ...props }: import("./search-field").SearchFieldRootProps) => import("react/jsx-runtime").JSX.Element) & {
    Root: ({ children, className, fullWidth, variant, ...props }: import("./search-field").SearchFieldRootProps) => import("react/jsx-runtime").JSX.Element;
    Group: ({ children, className, ...props }: import("./search-field").SearchFieldGroupProps) => import("react/jsx-runtime").JSX.Element;
    Input: ({ className, ...props }: import("./search-field").SearchFieldInputProps) => import("react/jsx-runtime").JSX.Element;
    SearchIcon: <E extends keyof React.JSX.IntrinsicElements = "svg">({ children, className, ...props }: import("./search-field").SearchFieldSearchIconProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./search-field").SearchFieldSearchIconProps<E>>) => import("react/jsx-runtime").JSX.Element;
    ClearButton: ({ className, ...props }: import("./search-field").SearchFieldClearButtonProps) => import("react/jsx-runtime").JSX.Element;
};
export type SearchField = {
    Props: ComponentProps<typeof SearchFieldRoot>;
    RootProps: ComponentProps<typeof SearchFieldRoot>;
    GroupProps: ComponentProps<typeof SearchFieldGroup>;
    InputProps: ComponentProps<typeof SearchFieldInput>;
    SearchIconProps: ComponentProps<typeof SearchFieldSearchIcon>;
    ClearButtonProps: ComponentProps<typeof SearchFieldClearButton>;
};
export { SearchFieldClearButton, SearchFieldGroup, SearchFieldInput, SearchFieldRoot, SearchFieldSearchIcon, };
export type { SearchFieldRootProps, SearchFieldRootProps as SearchFieldProps, SearchFieldGroupProps, SearchFieldInputProps, SearchFieldSearchIconProps, SearchFieldClearButtonProps, } from "./search-field";
export { searchFieldVariants } from "@heroui/styles";
export type { SearchFieldVariants } from "@heroui/styles";
