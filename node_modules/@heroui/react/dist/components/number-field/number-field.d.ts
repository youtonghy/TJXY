import type { NumberFieldVariants } from "@heroui/styles";
import type { ComponentPropsWithRef } from "react";
import { Button as ButtonPrimitive } from "react-aria-components/Button";
import { Group as GroupPrimitive } from "react-aria-components/Group";
import { Input as InputPrimitive } from "react-aria-components/Input";
import { NumberField as NumberFieldPrimitive } from "react-aria-components/NumberField";
interface NumberFieldRootProps extends ComponentPropsWithRef<typeof NumberFieldPrimitive>, NumberFieldVariants {
}
declare const NumberFieldRoot: ({ children, className, fullWidth, variant, ...props }: NumberFieldRootProps) => import("react/jsx-runtime").JSX.Element;
interface NumberFieldGroupProps extends ComponentPropsWithRef<typeof GroupPrimitive> {
}
declare const NumberFieldGroup: ({ children, className, ...props }: NumberFieldGroupProps) => import("react/jsx-runtime").JSX.Element;
interface NumberFieldInputProps extends ComponentPropsWithRef<typeof InputPrimitive> {
}
declare const NumberFieldInput: ({ className, ...props }: NumberFieldInputProps) => import("react/jsx-runtime").JSX.Element;
interface NumberFieldIncrementButtonProps extends ComponentPropsWithRef<typeof ButtonPrimitive> {
}
declare const NumberFieldIncrementButton: ({ children, className, ...props }: NumberFieldIncrementButtonProps) => import("react/jsx-runtime").JSX.Element;
interface NumberFieldDecrementButtonProps extends ComponentPropsWithRef<typeof ButtonPrimitive> {
}
declare const NumberFieldDecrementButton: ({ children, className, ...props }: NumberFieldDecrementButtonProps) => import("react/jsx-runtime").JSX.Element;
export { NumberFieldRoot, NumberFieldGroup, NumberFieldInput, NumberFieldIncrementButton, NumberFieldDecrementButton, };
export type { NumberFieldRootProps, NumberFieldGroupProps, NumberFieldInputProps, NumberFieldIncrementButtonProps, NumberFieldDecrementButtonProps, };
