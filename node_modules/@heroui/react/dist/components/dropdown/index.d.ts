import type { ComponentProps } from "react";
import { DropdownItem, DropdownItemIndicator, DropdownMenu, DropdownPopover, DropdownRoot, DropdownSection, DropdownSubmenuIndicator, DropdownSubmenuTrigger, DropdownTrigger } from "./dropdown";
export declare const Dropdown: (({ children, ...props }: import("./dropdown").DropdownRootProps) => import("react/jsx-runtime").JSX.Element) & {
    Root: ({ children, ...props }: import("./dropdown").DropdownRootProps) => import("react/jsx-runtime").JSX.Element;
    Trigger: ({ children, className, ...props }: import("./dropdown").DropdownTriggerProps) => import("react/jsx-runtime").JSX.Element;
    Popover: ({ children, className, placement, ...props }: import("./dropdown").DropdownPopoverProps) => import("react/jsx-runtime").JSX.Element;
    Menu: typeof DropdownMenu;
    Section: (props: import("./dropdown").DropdownSectionProps) => import("react/jsx-runtime").JSX.Element;
    Item: (props: import("./dropdown").DropdownItemProps) => import("react/jsx-runtime").JSX.Element;
    ItemIndicator: (props: import("./dropdown").DropdownItemIndicatorProps) => import("react/jsx-runtime").JSX.Element;
    SubmenuIndicator: (props: import("./dropdown").DropdownSubmenuIndicatorProps) => import("react/jsx-runtime").JSX.Element;
    SubmenuTrigger: ({ children, ...props }: import("./dropdown").DropdownSubmenuTriggerProps) => import("react/jsx-runtime").JSX.Element;
};
export type Dropdown<T extends object = object> = {
    Props: ComponentProps<typeof DropdownRoot>;
    RootProps: ComponentProps<typeof DropdownRoot>;
    TriggerProps: ComponentProps<typeof DropdownTrigger>;
    PopoverProps: ComponentProps<typeof DropdownPopover>;
    MenuProps: ComponentProps<typeof DropdownMenu<T>>;
    SectionProps: ComponentProps<typeof DropdownSection>;
    ItemProps: ComponentProps<typeof DropdownItem>;
    ItemIndicatorProps: ComponentProps<typeof DropdownItemIndicator>;
    SubmenuIndicatorProps: ComponentProps<typeof DropdownSubmenuIndicator>;
    SubmenuTriggerProps: ComponentProps<typeof DropdownSubmenuTrigger>;
};
export { DropdownItem, DropdownItemIndicator, DropdownMenu, DropdownPopover, DropdownRoot, DropdownSection, DropdownSubmenuIndicator, DropdownSubmenuTrigger, DropdownTrigger, };
export type { DropdownItemIndicatorProps, DropdownItemProps, DropdownMenuProps, DropdownPopoverProps, DropdownRootProps, DropdownRootProps as DropdownProps, DropdownSectionProps, DropdownSubmenuIndicatorProps, DropdownSubmenuTriggerProps, DropdownTriggerProps, } from "./dropdown";
export { dropdownVariants } from "@heroui/styles";
export type { DropdownVariants } from "@heroui/styles";
