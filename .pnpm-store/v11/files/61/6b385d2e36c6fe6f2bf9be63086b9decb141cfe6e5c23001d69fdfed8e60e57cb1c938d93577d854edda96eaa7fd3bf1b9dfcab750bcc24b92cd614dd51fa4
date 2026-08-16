"use client";
import { avatarVariants } from '@heroui/styles';
import * as AvatarPrimitive from '@radix-ui/react-avatar';
import React__default, { createContext } from 'react';
import { composeSlotClassName } from '../../utils/compose.js';
import { jsx } from 'react/jsx-runtime';

const AvatarContext = /*#__PURE__*/createContext({});

/* -------------------------------------------------------------------------------------------------
 * Avatar Root
 * -----------------------------------------------------------------------------------------------*/

const AvatarRoot = ({
  children,
  className,
  color,
  size,
  variant,
  ...props
}) => {
  const slots = React__default.useMemo(() => avatarVariants({
    color,
    size,
    variant
  }), [color, size, variant]);
  return /*#__PURE__*/jsx(AvatarContext, {
    value: {
      slots
    },
    children: /*#__PURE__*/jsx(AvatarPrimitive.Root, {
      className: slots.base({
        className
      }),
      ...props,
      children: children
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * Avatar Image
 * -----------------------------------------------------------------------------------------------*/

const AvatarImage = ({
  className,
  crossOrigin,
  loading,
  onError,
  onLoad,
  sizes,
  src,
  srcSet,
  ...props
}) => {
  const {
    slots
  } = React__default.use(AvatarContext);
  return /*#__PURE__*/jsx(AvatarPrimitive.Image, {
    className: composeSlotClassName(slots?.image, className),
    crossOrigin: crossOrigin,
    loading: loading,
    sizes: sizes,
    src: src,
    srcSet: srcSet,
    onError: onError,
    onLoad: onLoad,
    ...props
  });
};

/* -------------------------------------------------------------------------------------------------
 * Avatar Fallback
 * -----------------------------------------------------------------------------------------------*/

const AvatarFallback = ({
  className,
  color,
  ...props
}) => {
  const {
    slots
  } = React__default.use(AvatarContext);
  return /*#__PURE__*/jsx(AvatarPrimitive.Fallback, {
    className: composeSlotClassName(slots?.fallback, className, {
      color
    }),
    "data-slot": "avatar-fallback",
    ...props
  });
};

export { AvatarFallback, AvatarImage, AvatarRoot };
