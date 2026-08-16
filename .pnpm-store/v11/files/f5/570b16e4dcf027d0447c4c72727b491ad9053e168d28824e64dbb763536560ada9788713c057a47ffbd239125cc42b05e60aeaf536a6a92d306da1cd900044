import type { TagVariants } from "../tag";
import type { ComponentPropsWithRef } from "react";
import { tagGroupVariants } from "@heroui/styles";
import React from "react";
import { TagGroup as TagGroupPrimitive, TagList as TagListPrimitive } from "react-aria-components/TagGroup";
type TagGroupContext = {
    slots?: ReturnType<typeof tagGroupVariants>;
    size?: TagVariants["size"];
    variant?: TagVariants["variant"];
};
declare const TagGroupContext: React.Context<TagGroupContext>;
type TagGroupRootProps = ComponentPropsWithRef<typeof TagGroupPrimitive> & {
    size?: TagVariants["size"];
    variant?: TagVariants["variant"];
};
declare const TagGroupRoot: ({ children, className, size, variant, ...restProps }: TagGroupRootProps) => import("react/jsx-runtime").JSX.Element;
type TagGroupListProps<T extends object> = ComponentPropsWithRef<typeof TagListPrimitive<T>> & {};
declare const TagGroupList: <T extends object>({ children, className, ...restProps }: TagGroupListProps<T>) => import("react/jsx-runtime").JSX.Element;
export { TagGroupRoot, TagGroupList, TagGroupContext };
export type { TagGroupRootProps, TagGroupListProps };
