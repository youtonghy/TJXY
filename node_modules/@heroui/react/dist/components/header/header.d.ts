import type { ComponentPropsWithRef } from "react";
import { Header as HeaderPrimitive } from "react-aria-components/Header";
interface HeaderRootProps extends ComponentPropsWithRef<typeof HeaderPrimitive> {
}
declare const HeaderRoot: ({ children, className, ...rest }: HeaderRootProps) => import("react/jsx-runtime").JSX.Element;
export { HeaderRoot };
export type { HeaderRootProps };
