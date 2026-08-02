import type { ToolbarVariants } from "@heroui/styles";
import type { ComponentPropsWithRef } from "react";
import { Toolbar as ToolbarPrimitive } from "react-aria-components/Toolbar";
interface ToolbarRootProps extends ComponentPropsWithRef<typeof ToolbarPrimitive>, ToolbarVariants {
}
declare const ToolbarRoot: {
    ({ children, className, isAttached, orientation, ...props }: ToolbarRootProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
export { ToolbarRoot };
export type { ToolbarRootProps };
