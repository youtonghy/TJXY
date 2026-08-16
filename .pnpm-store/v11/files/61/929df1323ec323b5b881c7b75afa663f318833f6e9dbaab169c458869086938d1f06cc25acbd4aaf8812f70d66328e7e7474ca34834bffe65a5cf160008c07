import type { TypographyVariants } from "@heroui/styles";
import type { ComponentPropsWithRef, ReactNode } from "react";
import { Text as TextPrimitive } from "react-aria-components/Text";
type TypographyType = NonNullable<TypographyVariants["type"]>;
type TypographyAlign = NonNullable<TypographyVariants["align"]>;
type TypographyColor = NonNullable<TypographyVariants["color"]>;
type TypographyWeight = NonNullable<TypographyVariants["weight"]>;
interface TypographyRootProps extends Omit<ComponentPropsWithRef<typeof TextPrimitive>, "className" | "elementType"> {
    align?: TypographyAlign;
    children?: ReactNode;
    className?: string;
    color?: TypographyColor;
    type?: TypographyType;
    truncate?: boolean;
    weight?: TypographyWeight;
}
declare const TypographyRoot: ({ align, children, className, color, truncate, type, weight, ...props }: TypographyRootProps) => import("react/jsx-runtime").JSX.Element;
interface HeadingProps extends Omit<TypographyRootProps, "type"> {
    level?: 1 | 2 | 3 | 4 | 5 | 6;
}
declare const Heading: ({ level, ...props }: HeadingProps) => import("react/jsx-runtime").JSX.Element;
interface ParagraphProps extends Omit<TypographyRootProps, "type"> {
    size?: "base" | "sm" | "xs";
}
declare const Paragraph: ({ size, ...props }: ParagraphProps) => import("react/jsx-runtime").JSX.Element;
interface CodeProps extends Omit<TypographyRootProps, "type"> {
}
declare const Code: (props: CodeProps) => import("react/jsx-runtime").JSX.Element;
interface ProseProps extends Omit<ComponentPropsWithRef<"div">, "color"> {
    children: ReactNode;
}
declare const Prose: ({ children, className, ...props }: ProseProps) => import("react/jsx-runtime").JSX.Element;
export { Code, Heading, Paragraph, Prose, TypographyRoot };
export type { CodeProps, HeadingProps, ParagraphProps, ProseProps, TypographyRootProps };
