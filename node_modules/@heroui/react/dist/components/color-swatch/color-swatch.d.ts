import type { ColorSwatchVariants } from "@heroui/styles";
import type { ComponentPropsWithRef } from "react";
import { ColorSwatch as ColorSwatchPrimitive } from "react-aria-components/ColorSwatch";
interface ColorSwatchRootProps extends ComponentPropsWithRef<typeof ColorSwatchPrimitive>, ColorSwatchVariants {
}
declare const ColorSwatchRoot: ({ className, shape, size, style, ...props }: ColorSwatchRootProps) => import("react/jsx-runtime").JSX.Element;
export { ColorSwatchRoot };
export type { ColorSwatchRootProps };
