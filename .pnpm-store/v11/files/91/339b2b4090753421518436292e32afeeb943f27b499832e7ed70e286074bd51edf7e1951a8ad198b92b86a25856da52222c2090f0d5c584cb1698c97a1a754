import type { DOMRenderProps } from "../../utils/dom";
import type { SearchFieldVariants } from "@heroui/styles";
import type { ComponentPropsWithRef } from "react";
import React from "react";
import { Group as GroupPrimitive } from "react-aria-components/Group";
import { Input as InputPrimitive } from "react-aria-components/Input";
import { SearchField as SearchFieldPrimitive } from "react-aria-components/SearchField";
import { CloseButton } from "../close-button";
interface SearchFieldRootProps extends ComponentPropsWithRef<typeof SearchFieldPrimitive>, SearchFieldVariants {
}
declare const SearchFieldRoot: ({ children, className, fullWidth, variant, ...props }: SearchFieldRootProps) => import("react/jsx-runtime").JSX.Element;
interface SearchFieldGroupProps extends ComponentPropsWithRef<typeof GroupPrimitive> {
}
declare const SearchFieldGroup: ({ children, className, ...props }: SearchFieldGroupProps) => import("react/jsx-runtime").JSX.Element;
interface SearchFieldInputProps extends ComponentPropsWithRef<typeof InputPrimitive> {
}
declare const SearchFieldInput: ({ className, ...props }: SearchFieldInputProps) => import("react/jsx-runtime").JSX.Element;
interface SearchFieldSearchIconProps<E extends keyof React.JSX.IntrinsicElements = "svg"> extends DOMRenderProps<E, undefined> {
    children?: React.ReactNode;
    className?: string;
}
declare const SearchFieldSearchIcon: <E extends keyof React.JSX.IntrinsicElements = "svg">({ children, className, ...props }: SearchFieldSearchIconProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof SearchFieldSearchIconProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface SearchFieldClearButtonProps extends ComponentPropsWithRef<typeof CloseButton> {
}
declare const SearchFieldClearButton: ({ className, ...props }: SearchFieldClearButtonProps) => import("react/jsx-runtime").JSX.Element;
export { SearchFieldRoot, SearchFieldGroup, SearchFieldInput, SearchFieldSearchIcon, SearchFieldClearButton, };
export type { SearchFieldRootProps, SearchFieldGroupProps, SearchFieldInputProps, SearchFieldSearchIconProps, SearchFieldClearButtonProps, };
