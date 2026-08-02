import type { ErrorMessageVariants } from "@heroui/styles";
import type { ComponentPropsWithRef } from "react";
import type { TextProps } from "react-aria-components/Text";
import { Text } from "react-aria-components/Text";
interface ErrorMessageRootProps extends ComponentPropsWithRef<typeof Text>, TextProps, ErrorMessageVariants {
}
declare const ErrorMessageRoot: ({ children, className, ...rest }: ErrorMessageRootProps) => import("react/jsx-runtime").JSX.Element | null;
export { ErrorMessageRoot };
export type { ErrorMessageRootProps };
