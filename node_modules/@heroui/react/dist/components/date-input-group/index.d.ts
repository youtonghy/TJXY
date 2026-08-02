import type { ComponentProps } from "react";
import { DateInputGroupInput, DateInputGroupInputContainer, DateInputGroupPrefix, DateInputGroupRoot, DateInputGroupSegment, DateInputGroupSuffix } from "./date-input-group";
export declare const DateInputGroup: (({ children, className, fullWidth, variant, ...props }: import("./date-input-group").DateInputGroupRootProps) => import("react/jsx-runtime").JSX.Element) & {
    Root: ({ children, className, fullWidth, variant, ...props }: import("./date-input-group").DateInputGroupRootProps) => import("react/jsx-runtime").JSX.Element;
    Input: ({ className, ...props }: import("./date-input-group").DateInputGroupInputProps) => import("react/jsx-runtime").JSX.Element;
    InputContainer: <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...props }: import("./date-input-group").DateInputGroupInputContainerProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./date-input-group").DateInputGroupInputContainerProps<E>>) => import("react/jsx-runtime").JSX.Element;
    Segment: ({ className, segment, ...props }: import("./date-input-group").DateInputGroupSegmentProps) => import("react/jsx-runtime").JSX.Element;
    Prefix: <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...props }: import("./date-input-group").DateInputGroupPrefixProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./date-input-group").DateInputGroupPrefixProps<E>>) => import("react/jsx-runtime").JSX.Element;
    Suffix: <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...props }: import("./date-input-group").DateInputGroupSuffixProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./date-input-group").DateInputGroupSuffixProps<E>>) => import("react/jsx-runtime").JSX.Element;
};
export type DateInputGroup = {
    Props: ComponentProps<typeof DateInputGroupRoot>;
    RootProps: ComponentProps<typeof DateInputGroupRoot>;
    InputProps: ComponentProps<typeof DateInputGroupInput>;
    InputContainerProps: ComponentProps<typeof DateInputGroupInputContainer>;
    SegmentProps: ComponentProps<typeof DateInputGroupSegment>;
    PrefixProps: ComponentProps<typeof DateInputGroupPrefix>;
    SuffixProps: ComponentProps<typeof DateInputGroupSuffix>;
};
export { DateInputGroupInput, DateInputGroupInputContainer, DateInputGroupPrefix, DateInputGroupRoot, DateInputGroupSegment, DateInputGroupSuffix, };
export type { DateInputGroupRootProps, DateInputGroupRootProps as DateInputGroupProps, DateInputGroupInputProps, DateInputGroupInputContainerProps, DateInputGroupSegmentProps, DateInputGroupPrefixProps, DateInputGroupSuffixProps, } from "./date-input-group";
export { dateInputGroupVariants } from "@heroui/styles";
export type { DateInputGroupVariants } from "@heroui/styles";
