import type { ComponentProps } from "react";
import { InputOTPGroup, InputOTPRoot, InputOTPSeparator, InputOTPSlot } from "./input-otp";
export declare const InputOTP: (({ className, inputClassName, isDisabled, isInvalid, validationDetails, validationErrors, variant, ...props }: import("./input-otp").InputOTPRootProps) => import("react/jsx-runtime").JSX.Element) & {
    Root: ({ className, inputClassName, isDisabled, isInvalid, validationDetails, validationErrors, variant, ...props }: import("./input-otp").InputOTPRootProps) => import("react/jsx-runtime").JSX.Element;
    Group: ({ className, ...props }: import("./input-otp").InputOTPGroupProps) => import("react/jsx-runtime").JSX.Element;
    Slot: ({ className, index, ...props }: import("./input-otp").InputOTPSlotProps) => import("react/jsx-runtime").JSX.Element;
    Separator: ({ className, ...props }: import("./input-otp").InputOTPSeparatorProps) => import("react/jsx-runtime").JSX.Element;
};
export type InputOTP = {
    Props: ComponentProps<typeof InputOTPRoot>;
    RootProps: ComponentProps<typeof InputOTPRoot>;
    GroupProps: ComponentProps<typeof InputOTPGroup>;
    SlotProps: ComponentProps<typeof InputOTPSlot>;
    SeparatorProps: ComponentProps<typeof InputOTPSeparator>;
};
export { InputOTPRoot, InputOTPGroup, InputOTPSlot, InputOTPSeparator };
export type { InputOTPRootProps, InputOTPRootProps as InputOTPProps, InputOTPGroupProps, InputOTPSlotProps, InputOTPSeparatorProps, } from "./input-otp";
export { inputOTPVariants } from "@heroui/styles";
export type { InputOTPVariants } from "@heroui/styles";
export declare const REGEXP_ONLY_DIGITS = "^\\d+$";
export declare const REGEXP_ONLY_CHARS = "^[a-zA-Z]+$";
export declare const REGEXP_ONLY_DIGITS_AND_CHARS = "^[a-zA-Z0-9]+$";
