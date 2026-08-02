import type { ComponentPropsWithRef } from "react";
import { ListBoxSection as ListBoxSectionPrimitive } from "react-aria-components/ListBox";
interface ListBoxSectionRootProps extends ComponentPropsWithRef<typeof ListBoxSectionPrimitive> {
    className?: string;
}
declare const ListBoxSectionRoot: ({ children, className, ...props }: ListBoxSectionRootProps) => import("react/jsx-runtime").JSX.Element;
export { ListBoxSectionRoot };
export type { ListBoxSectionRootProps };
