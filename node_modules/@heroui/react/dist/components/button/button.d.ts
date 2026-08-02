import type { ButtonVariants } from "@heroui/styles";
import type { ComponentPropsWithRef } from "react";
import { Button as ButtonPrimitive } from "react-aria-components/Button";
import { BUTTON_GROUP_CHILD } from "../button-group";
interface ButtonRootProps extends ComponentPropsWithRef<typeof ButtonPrimitive>, ButtonVariants {
    [BUTTON_GROUP_CHILD]?: boolean;
}
declare const ButtonRoot: ({ children, className, fullWidth, isDisabled, isIconOnly, size, slot, style, variant, [BUTTON_GROUP_CHILD]: isButtonGroupChild, ...rest }: ButtonRootProps) => import("react/jsx-runtime").JSX.Element;
export { ButtonRoot };
export type { ButtonRootProps };
