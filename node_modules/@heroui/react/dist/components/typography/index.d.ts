import type { ComponentProps } from "react";
import { Code, Heading, Paragraph, Prose, TypographyRoot } from "./typography";
export declare const Typography: (({ align, children, className, color, truncate, type, weight, ...props }: import("./typography").TypographyRootProps) => import("react/jsx-runtime").JSX.Element) & {
    Code: (props: import("./typography").CodeProps) => import("react/jsx-runtime").JSX.Element;
    Heading: ({ level, ...props }: import("./typography").HeadingProps) => import("react/jsx-runtime").JSX.Element;
    Paragraph: ({ size, ...props }: import("./typography").ParagraphProps) => import("react/jsx-runtime").JSX.Element;
    Prose: ({ children, className, ...props }: import("./typography").ProseProps) => import("react/jsx-runtime").JSX.Element;
    Root: ({ align, children, className, color, truncate, type, weight, ...props }: import("./typography").TypographyRootProps) => import("react/jsx-runtime").JSX.Element;
};
export type Typography = {
    CodeProps: ComponentProps<typeof Code>;
    HeadingProps: ComponentProps<typeof Heading>;
    ParagraphProps: ComponentProps<typeof Paragraph>;
    ProseProps: ComponentProps<typeof Prose>;
    Props: ComponentProps<typeof TypographyRoot>;
    RootProps: ComponentProps<typeof TypographyRoot>;
};
export { Code, Heading, Paragraph, Prose, TypographyRoot };
export type { CodeProps, HeadingProps, ParagraphProps, ProseProps, TypographyRootProps, TypographyRootProps as TypographyProps, } from "./typography";
export { typographyVariants } from "@heroui/styles";
export type { TypographyVariants } from "@heroui/styles";
