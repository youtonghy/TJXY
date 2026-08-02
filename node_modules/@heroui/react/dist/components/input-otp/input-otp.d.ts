import type { InputOTPVariants } from "@heroui/styles";
import type { ComponentPropsWithRef } from "react";
import { OTPInput } from "input-otp";
import React from "react";
interface InputOTPRootProps extends Omit<ComponentPropsWithRef<typeof OTPInput>, "disabled" | "containerClassName" | "render">, InputOTPVariants {
    isDisabled?: boolean;
    isInvalid?: boolean;
    validationErrors?: string[];
    validationDetails?: ValidityState;
    inputClassName?: string;
    children: React.ReactNode;
}
declare const InputOTPRoot: ({ className, inputClassName, isDisabled, isInvalid, validationDetails, validationErrors, variant, ...props }: InputOTPRootProps) => import("react/jsx-runtime").JSX.Element;
interface InputOTPGroupProps extends ComponentPropsWithRef<"div"> {
}
declare const InputOTPGroup: ({ className, ...props }: InputOTPGroupProps) => import("react/jsx-runtime").JSX.Element;
interface InputOTPSlotProps extends ComponentPropsWithRef<"div"> {
    index: number;
}
declare const InputOTPSlot: ({ className, index, ...props }: InputOTPSlotProps) => import("react/jsx-runtime").JSX.Element;
interface InputOTPSeparatorProps extends ComponentPropsWithRef<"div"> {
    className?: string;
}
declare const InputOTPSeparator: ({ className, ...props }: InputOTPSeparatorProps) => import("react/jsx-runtime").JSX.Element;
export { InputOTPRoot, InputOTPGroup, InputOTPSlot, InputOTPSeparator };
export type { InputOTPRootProps, InputOTPGroupProps, InputOTPSlotProps, InputOTPSeparatorProps };
