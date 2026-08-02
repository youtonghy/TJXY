import type { SeparatorVariants } from "@heroui/styles";
import type { ComponentPropsWithRef } from "react";
import { Separator as SeparatorPrimitive } from "react-aria-components/Separator";
interface SeparatorRootProps extends ComponentPropsWithRef<typeof SeparatorPrimitive>, SeparatorVariants {
}
declare const SeparatorRoot: ({ className, orientation, variant, ...props }: SeparatorRootProps) => import("react/jsx-runtime").JSX.Element;
export { SeparatorRoot };
export type { SeparatorRootProps };
