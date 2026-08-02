import type { ColorAreaVariants } from "@heroui/styles";
import type { ComponentPropsWithRef } from "react";
import { ColorArea as ColorAreaPrimitive, ColorThumb as ColorThumbPrimitive } from "react-aria-components/ColorArea";
interface ColorAreaRootProps extends ComponentPropsWithRef<typeof ColorAreaPrimitive>, ColorAreaVariants {
}
declare const ColorAreaRoot: ({ children, className, showDots, style, ...props }: ColorAreaRootProps) => import("react/jsx-runtime").JSX.Element;
interface ColorAreaThumbProps extends ComponentPropsWithRef<typeof ColorThumbPrimitive> {
}
declare const ColorAreaThumb: ({ className, style, ...props }: ColorAreaThumbProps) => import("react/jsx-runtime").JSX.Element;
export { ColorAreaRoot, ColorAreaThumb };
export type { ColorAreaRootProps, ColorAreaThumbProps };
