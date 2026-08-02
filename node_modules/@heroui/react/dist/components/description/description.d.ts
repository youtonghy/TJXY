import type { DescriptionVariants } from "@heroui/styles";
import type { ComponentPropsWithRef } from "react";
import type { TextProps } from "react-aria-components/Text";
import { Text } from "react-aria-components/Text";
interface DescriptionRootProps extends ComponentPropsWithRef<typeof Text>, TextProps, DescriptionVariants {
}
declare const DescriptionRoot: ({ children, className, ...rest }: DescriptionRootProps) => import("react/jsx-runtime").JSX.Element | null;
export { DescriptionRoot };
export type { DescriptionRootProps };
