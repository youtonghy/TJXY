import type { ComponentProps } from "react";
import { SeparatorRoot } from "./separator";
export declare const Separator: (({ className, orientation, variant, ...props }: import("./separator").SeparatorRootProps) => import("react/jsx-runtime").JSX.Element) & {
    Root: ({ className, orientation, variant, ...props }: import("./separator").SeparatorRootProps) => import("react/jsx-runtime").JSX.Element;
};
export type Separator = {
    Props: ComponentProps<typeof SeparatorRoot>;
    RootProps: ComponentProps<typeof SeparatorRoot>;
};
export { SeparatorRoot };
export type { SeparatorRootProps, SeparatorRootProps as SeparatorProps } from "./separator";
export { separatorVariants } from "@heroui/styles";
export type { SeparatorVariants } from "@heroui/styles";
