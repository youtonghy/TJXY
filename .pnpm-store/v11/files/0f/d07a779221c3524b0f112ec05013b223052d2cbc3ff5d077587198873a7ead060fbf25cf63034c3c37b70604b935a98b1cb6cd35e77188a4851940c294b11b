"use client";
import { disclosureVariants } from '@heroui/styles';
import React__default, { createContext, use, useRef } from 'react';
import { Button } from 'react-aria-components/Button';
import { Disclosure, DisclosureStateContext, DisclosurePanel } from 'react-aria-components/Disclosure';
import { Heading } from 'react-aria-components/Heading';
import { dataAttr } from '../../utils/assertion.js';
import { composeTwRenderProps, composeSlotClassName } from '../../utils/compose.js';
import { dom } from '../../utils/dom.js';
import { IconChevronDown } from '../icons.js';
import { jsx, Fragment } from 'react/jsx-runtime';

const DisclosureContext = /*#__PURE__*/createContext({});

/* -------------------------------------------------------------------------------------------------
 * Disclosure Root
 * -----------------------------------------------------------------------------------------------*/

const DisclosureRoot = ({
  children,
  className,
  ...props
}) => {
  const slots = React__default.useMemo(() => disclosureVariants({}), []);
  return /*#__PURE__*/jsx(DisclosureContext, {
    value: {
      slots
    },
    children: /*#__PURE__*/jsx(Disclosure, {
      "data-slot": "disclosure",
      ...props,
      className: composeTwRenderProps(className, slots.base()),
      children: values => /*#__PURE__*/jsx(Fragment, {
        children: typeof children === "function" ? children(values) : children
      })
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * Disclosure Heading
 * -----------------------------------------------------------------------------------------------*/

const DisclosureHeading = ({
  className,
  ...props
}) => {
  const {
    slots
  } = use(DisclosureContext);
  return /*#__PURE__*/jsx(Heading, {
    className: composeSlotClassName(slots?.heading, className),
    "data-slot": "disclosure-heading",
    ...props
  });
};

/* -------------------------------------------------------------------------------------------------
 * Disclosure Trigger
 * -----------------------------------------------------------------------------------------------*/

const DisclosureTrigger = ({
  className,
  ...props
}) => {
  const {
    slots
  } = use(DisclosureContext);
  return /*#__PURE__*/jsx(Button, {
    className: composeTwRenderProps(className, slots?.trigger()),
    "data-slot": "disclosure-trigger",
    slot: "trigger",
    ...props,
    children: values => /*#__PURE__*/jsx(Fragment, {
      children: typeof props.children === "function" ? props.children(values) : props.children
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * Disclosure Content
 * -----------------------------------------------------------------------------------------------*/

const DisclosureContent = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(DisclosureContext);
  const contentRef = useRef(null);
  const {
    isExpanded
  } = use(DisclosureStateContext);
  return /*#__PURE__*/jsx(DisclosurePanel, {
    ref: contentRef,
    className: composeTwRenderProps(className, slots?.content()),
    "data-expanded": dataAttr(isExpanded),
    "data-slot": "disclosure-content",
    ...props,
    children: children
  });
};

/* -------------------------------------------------------------------------------------------------
 * Disclosure Body
 * -----------------------------------------------------------------------------------------------*/

const DisclosureBody = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(DisclosureContext);
  return /*#__PURE__*/jsx(dom.div, {
    className: slots?.body({}),
    "data-slot": "disclosure-body",
    ...props,
    children: /*#__PURE__*/jsx("div", {
      className: composeSlotClassName(slots?.bodyInner, className),
      children: children
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * Disclosure Indicator
 * -----------------------------------------------------------------------------------------------*/

const DisclosureIndicator = ({
  children,
  className,
  ...props
}) => {
  const {
    isExpanded
  } = use(DisclosureStateContext);
  const {
    slots
  } = use(DisclosureContext);
  if (children && /*#__PURE__*/React__default.isValidElement(children)) {
    return /*#__PURE__*/React__default.cloneElement(children, {
      ...props,
      "data-expanded": dataAttr(isExpanded),
      className: composeSlotClassName(slots?.indicator, className),
      "data-slot": "disclosure-indicator"
    });
  }
  return /*#__PURE__*/jsx(IconChevronDown, {
    className: composeSlotClassName(slots?.indicator, className),
    "data-expanded": dataAttr(isExpanded),
    "data-slot": "disclosure-indicator",
    ...props
  });
};

export { DisclosureBody, DisclosureContent, DisclosureHeading, DisclosureIndicator, DisclosureRoot, DisclosureTrigger };
