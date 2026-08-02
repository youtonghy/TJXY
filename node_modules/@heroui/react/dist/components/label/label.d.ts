import type { LabelVariants } from "@heroui/styles";
import type { ComponentPropsWithRef } from "react";
import { Label as LabelPrimitive } from "react-aria-components/Label";
interface LabelRootProps extends ComponentPropsWithRef<typeof LabelPrimitive>, LabelVariants {
}
declare const LabelRoot: ({ children, className, isDisabled, isInvalid, isRequired, ...rest }: LabelRootProps) => import("react/jsx-runtime").JSX.Element;
export { LabelRoot };
export type { LabelRootProps };
