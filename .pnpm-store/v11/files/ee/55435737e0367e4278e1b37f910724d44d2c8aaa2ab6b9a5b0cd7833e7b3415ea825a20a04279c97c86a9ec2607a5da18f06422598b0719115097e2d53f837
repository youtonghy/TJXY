import type { DropdownVariants } from "@heroui/styles";
import type { ComponentPropsWithRef } from "react";
import React from "react";
import { Button } from "react-aria-components/Button";
import { Menu as MenuPrimitive, MenuTrigger as MenuTriggerPrimitive, Popover as PopoverPrimitive, SubmenuTrigger as SubmenuTriggerPrimitive } from "react-aria-components/Menu";
import { MenuItemIndicator, MenuItemRoot, MenuItemSubmenuIndicator } from "../menu-item";
import { MenuSectionRoot } from "../menu-section";
interface DropdownRootProps extends ComponentPropsWithRef<typeof MenuTriggerPrimitive>, DropdownVariants {
    className?: string;
}
declare const DropdownRoot: ({ children, ...props }: DropdownRootProps) => import("react/jsx-runtime").JSX.Element;
interface DropdownTriggerProps extends ComponentPropsWithRef<typeof Button> {
}
declare const DropdownTrigger: ({ children, className, ...props }: DropdownTriggerProps) => import("react/jsx-runtime").JSX.Element;
interface DropdownPopoverProps extends Omit<ComponentPropsWithRef<typeof PopoverPrimitive>, "children">, DropdownVariants {
    children: React.ReactNode;
}
declare const DropdownPopover: ({ children, className, placement, ...props }: DropdownPopoverProps) => import("react/jsx-runtime").JSX.Element;
interface DropdownMenuProps<T extends object> extends ComponentPropsWithRef<typeof MenuPrimitive<T>>, DropdownVariants {
    className?: string;
}
declare function DropdownMenu<T extends object>({ className, ...props }: DropdownMenuProps<T>): import("react/jsx-runtime").JSX.Element;
interface DropdownItemProps extends ComponentPropsWithRef<typeof MenuItemRoot> {
}
declare const DropdownItem: (props: DropdownItemProps) => import("react/jsx-runtime").JSX.Element;
interface DropdownSubmenuIndicatorProps extends ComponentPropsWithRef<typeof MenuItemSubmenuIndicator> {
}
declare const DropdownSubmenuIndicator: (props: DropdownSubmenuIndicatorProps) => import("react/jsx-runtime").JSX.Element;
interface DropdownSubmenuTriggerProps extends ComponentPropsWithRef<typeof SubmenuTriggerPrimitive> {
}
declare const DropdownSubmenuTrigger: ({ children, ...props }: DropdownSubmenuTriggerProps) => import("react/jsx-runtime").JSX.Element;
interface DropdownItemIndicatorProps extends ComponentPropsWithRef<typeof MenuItemIndicator> {
}
declare const DropdownItemIndicator: (props: DropdownItemIndicatorProps) => import("react/jsx-runtime").JSX.Element;
interface DropdownSectionProps extends ComponentPropsWithRef<typeof MenuSectionRoot> {
}
declare const DropdownSection: (props: DropdownSectionProps) => import("react/jsx-runtime").JSX.Element;
export { DropdownItem, DropdownItemIndicator, DropdownMenu, DropdownPopover, DropdownRoot, DropdownSection, DropdownSubmenuIndicator, DropdownSubmenuTrigger, DropdownTrigger, };
export type { DropdownItemIndicatorProps, DropdownItemProps, DropdownMenuProps, DropdownPopoverProps, DropdownRootProps, DropdownSectionProps, DropdownSubmenuIndicatorProps, DropdownSubmenuTriggerProps, DropdownTriggerProps, };
