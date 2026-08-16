"use client";
import { inputOTPVariants } from '@heroui/styles';
import { OTPInput, OTPInputContext } from 'input-otp';
import React__default, { createContext, use } from 'react';
import { FieldErrorContext } from 'react-aria-components/FieldError';
import { dataAttr } from '../../utils/assertion.js';
import { composeSlotClassName } from '../../utils/compose.js';
import { jsx, jsxs } from 'react/jsx-runtime';

const InputOTPContext = /*#__PURE__*/createContext({
  isDisabled: false,
  isInvalid: false
});

/* -------------------------------------------------------------------------------------------------
 * Input OTP Root
 * -----------------------------------------------------------------------------------------------*/

const InputOTPRoot = ({
  className,
  inputClassName,
  isDisabled = false,
  isInvalid = false,
  validationDetails,
  validationErrors = [],
  variant,
  ...props
}) => {
  const slots = React__default.useMemo(() => inputOTPVariants({
    variant
  }), [variant]);
  const validation = React__default.useMemo(() => ({
    isInvalid,
    validationErrors,
    validationDetails
  }), [isInvalid, validationErrors, validationDetails]);
  return /*#__PURE__*/jsx(InputOTPContext, {
    value: {
      slots,
      isDisabled,
      isInvalid
    },
    children: /*#__PURE__*/jsx(FieldErrorContext, {
      value: validation,
      children: /*#__PURE__*/jsx(OTPInput
      // OTP Input package uses the `className` prop for the actual `input` element which is not visible to the user so no need to pass it to the base container
      , {
        className: slots.input({
          className: inputClassName
        }),
        containerClassName: slots.base({
          className
        }),
        "data-disabled": dataAttr(isDisabled),
        "data-invalid": dataAttr(isInvalid),
        "data-slot": "input-otp",
        disabled: isDisabled,
        ...props
      })
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * Input OTP Group
 * -----------------------------------------------------------------------------------------------*/

const InputOTPGroup = ({
  className,
  ...props
}) => {
  const {
    slots
  } = use(InputOTPContext);
  return /*#__PURE__*/jsx("div", {
    className: composeSlotClassName(slots?.group, className),
    "data-slot": "input-otp-group",
    ...props
  });
};

/* -------------------------------------------------------------------------------------------------
 * Input OTP Slot
 * -----------------------------------------------------------------------------------------------*/

const InputOTPSlot = ({
  className,
  index,
  ...props
}) => {
  const {
    isDisabled,
    isInvalid,
    slots
  } = use(InputOTPContext);
  const inputOTPContext = use(OTPInputContext);
  const {
    char,
    hasFakeCaret,
    isActive
  } = inputOTPContext?.slots[index] ?? {};
  return /*#__PURE__*/jsxs("div", {
    ...props,
    className: composeSlotClassName(slots?.slot, className),
    "data-active": dataAttr(isActive),
    "data-disabled": dataAttr(isDisabled),
    "data-filled": dataAttr(!!char),
    "data-invalid": dataAttr(isInvalid),
    "data-slot": "input-otp-slot",
    children: [char ? /*#__PURE__*/jsx("div", {
      className: slots?.slotValue(),
      "data-slot": "input-otp-slot-value",
      children: char
    }) : null, hasFakeCaret && isActive ? /*#__PURE__*/jsx("div", {
      className: slots?.caret(),
      "data-slot": "input-otp-caret"
    }) : null]
  });
};

/* -------------------------------------------------------------------------------------------------
 * Input OTP Separator
 * -----------------------------------------------------------------------------------------------*/

const InputOTPSeparator = ({
  className,
  ...props
}) => {
  const {
    slots
  } = use(InputOTPContext);
  return /*#__PURE__*/jsx("div", {
    className: composeSlotClassName(slots?.separator, className),
    "data-slot": "input-otp-separator",
    ...props
  });
};

export { InputOTPGroup, InputOTPRoot, InputOTPSeparator, InputOTPSlot };
