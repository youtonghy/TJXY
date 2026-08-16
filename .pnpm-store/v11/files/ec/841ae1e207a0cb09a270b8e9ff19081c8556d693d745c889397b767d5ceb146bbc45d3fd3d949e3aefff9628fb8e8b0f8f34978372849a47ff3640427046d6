"use client";
import { surfaceVariants } from '@heroui/styles';
import { createContext } from 'react';
import { dom } from '../../utils/dom.js';
import { jsx } from 'react/jsx-runtime';

const SurfaceContext = /*#__PURE__*/createContext({});

/* ------------------------------------------------------------------------------------------------
 * Surface Root
 * --------------------------------------------------------------------------------------------- */

const SurfaceRoot = ({
  children,
  className,
  variant = "default",
  ...rest
}) => {
  return /*#__PURE__*/jsx(SurfaceContext, {
    value: {
      variant
    },
    children: /*#__PURE__*/jsx(dom.div, {
      className: surfaceVariants({
        variant,
        className
      }),
      "data-slot": "surface",
      ...rest,
      children: children
    })
  });
};

export { SurfaceContext, SurfaceRoot };
