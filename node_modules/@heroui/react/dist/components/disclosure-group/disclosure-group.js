"use client";
import { disclosureGroupVariants } from '@heroui/styles';
import React__default, { createContext } from 'react';
import { DisclosureGroup } from 'react-aria-components/DisclosureGroup';
import { composeTwRenderProps } from '../../utils/compose.js';
import { jsx, Fragment } from 'react/jsx-runtime';

const DisclosureGroupContext = /*#__PURE__*/createContext({});

/* -------------------------------------------------------------------------------------------------
 * Disclosure Group Root
 * -----------------------------------------------------------------------------------------------*/

const DisclosureGroupRoot = ({
  children,
  className,
  ...props
}) => {
  const slots = React__default.useMemo(() => disclosureGroupVariants({}), []);
  return /*#__PURE__*/jsx(DisclosureGroupContext, {
    value: {
      slots
    },
    children: /*#__PURE__*/jsx(DisclosureGroup, {
      "data-slot": "disclosure-group",
      ...props,
      className: composeTwRenderProps(className, slots.base()),
      children: values => /*#__PURE__*/jsx(Fragment, {
        children: typeof children === "function" ? children(values) : children
      })
    })
  });
};

export { DisclosureGroupRoot };
