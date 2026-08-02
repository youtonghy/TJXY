import type { ComponentProps } from "react";
import { HeaderRoot } from "./header";
export declare const Header: ({ children, className, ...rest }: import("./header").HeaderRootProps) => import("react/jsx-runtime").JSX.Element;
export type Header = {
    Props: ComponentProps<typeof HeaderRoot>;
};
export { HeaderRoot };
export type { HeaderRootProps, HeaderRootProps as HeaderProps } from "./header";
export { headerVariants } from "@heroui/styles";
