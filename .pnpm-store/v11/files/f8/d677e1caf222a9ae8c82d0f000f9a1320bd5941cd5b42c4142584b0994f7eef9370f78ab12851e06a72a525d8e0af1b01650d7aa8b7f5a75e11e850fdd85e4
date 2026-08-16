"use client";
import { cardVariants } from '@heroui/styles';
import React__default, { createContext, use } from 'react';
import { composeSlotClassName } from '../../utils/compose.js';
import { dom } from '../../utils/dom.js';
import { jsx } from 'react/jsx-runtime';
import { SurfaceContext } from '../surface/surface.js';

const CardContext = /*#__PURE__*/createContext({});

/* -------------------------------------------------------------------------------------------------
 * Card Root
 * -----------------------------------------------------------------------------------------------*/

const CardRoot = ({
  children,
  className,
  variant = "default",
  ...props
}) => {
  const slots = React__default.useMemo(() => cardVariants({
    variant
  }), [variant]);
  const content = /*#__PURE__*/jsx(dom.div, {
    className: slots.base({
      className
    }),
    "data-slot": "card",
    ...props,
    children: children
  });
  return /*#__PURE__*/jsx(CardContext, {
    value: {
      slots
    },
    children: variant === "transparent" ? content :
    /*#__PURE__*/
    // Allows inner components to apply "on-surface" colors for proper contrast
    jsx(SurfaceContext, {
      value: {
        variant: variant
      },
      children: content
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * Card Header
 * -----------------------------------------------------------------------------------------------*/

const CardHeader = ({
  className,
  ...props
}) => {
  const {
    slots
  } = use(CardContext);
  return /*#__PURE__*/jsx(dom.div, {
    className: composeSlotClassName(slots?.header, className),
    "data-slot": "card-header",
    ...props
  });
};

/* -------------------------------------------------------------------------------------------------
 * Card Title
 * -----------------------------------------------------------------------------------------------*/

const CardTitle = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(CardContext);
  return /*#__PURE__*/jsx(dom.h3, {
    className: composeSlotClassName(slots?.title, className),
    "data-slot": "card-title",
    ...props,
    children: children
  });
};

/* -------------------------------------------------------------------------------------------------
 * Card Description
 * -----------------------------------------------------------------------------------------------*/

const CardDescription = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(CardContext);
  return /*#__PURE__*/jsx(dom.p, {
    className: composeSlotClassName(slots?.description, className),
    "data-slot": "card-description",
    ...props,
    children: children
  });
};

/* -------------------------------------------------------------------------------------------------
 * Card Content
 * -----------------------------------------------------------------------------------------------*/

const CardContent = ({
  className,
  ...props
}) => {
  const {
    slots
  } = use(CardContext);
  return /*#__PURE__*/jsx(dom.div, {
    className: composeSlotClassName(slots?.content, className),
    "data-slot": "card-content",
    ...props
  });
};

/* -------------------------------------------------------------------------------------------------
 * Card Footer
 * -----------------------------------------------------------------------------------------------*/

const CardFooter = ({
  className,
  ...props
}) => {
  const {
    slots
  } = use(CardContext);
  return /*#__PURE__*/jsx(dom.div, {
    className: composeSlotClassName(slots?.footer, className),
    "data-slot": "card-footer",
    ...props
  });
};

export { CardContent, CardDescription, CardFooter, CardHeader, CardRoot, CardTitle };
