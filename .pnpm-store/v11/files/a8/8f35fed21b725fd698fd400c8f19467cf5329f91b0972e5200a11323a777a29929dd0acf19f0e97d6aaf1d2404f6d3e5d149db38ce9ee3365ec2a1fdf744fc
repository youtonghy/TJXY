import type { ColorPickerVariants } from "@heroui/styles";
import type { ComponentPropsWithRef } from "react";
import type { ColorPickerProps as ColorPickerPrimitiveProps } from "react-aria-components/ColorPicker";
import React from "react";
import { Button as ButtonPrimitive } from "react-aria-components/Button";
import { Popover as PopoverPrimitive } from "react-aria-components/Popover";
interface ColorPickerRootProps extends Omit<ColorPickerPrimitiveProps, "children">, ColorPickerVariants {
    /** Additional class name for the wrapper element */
    className?: string;
    /** Content of the color picker (Trigger, Popover, etc.) */
    children: React.ReactNode;
}
declare const ColorPickerRoot: ({ children, className, ...props }: ColorPickerRootProps) => import("react/jsx-runtime").JSX.Element;
interface ColorPickerTriggerProps extends ComponentPropsWithRef<typeof ButtonPrimitive> {
}
declare const ColorPickerTrigger: ({ children, className, ...props }: ColorPickerTriggerProps) => import("react/jsx-runtime").JSX.Element;
interface ColorPickerPopoverProps extends Omit<ComponentPropsWithRef<typeof PopoverPrimitive>, "children"> {
    children: React.ReactNode;
}
declare const ColorPickerPopover: ({ children, className, placement, ...props }: ColorPickerPopoverProps) => import("react/jsx-runtime").JSX.Element;
export { ColorPickerRoot, ColorPickerTrigger, ColorPickerPopover };
export type { ColorPickerRootProps, ColorPickerTriggerProps, ColorPickerPopoverProps };
