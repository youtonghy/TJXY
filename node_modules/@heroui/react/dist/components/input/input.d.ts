import type { InputVariants } from "@heroui/styles";
import type { ComponentPropsWithRef } from "react";
import { Input as InputPrimitive } from "react-aria-components/Input";
interface InputRootProps extends ComponentPropsWithRef<typeof InputPrimitive>, InputVariants {
}
declare const InputRoot: ({ className, fullWidth, variant: variantProp, ...rest }: InputRootProps) => import("react/jsx-runtime").JSX.Element;
export { InputRoot };
export type { InputRootProps };
