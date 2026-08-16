"use client";
import { accordionVariants } from '@heroui/styles';
import React__default, { createContext, use } from 'react';
import { Button } from 'react-aria-components/Button';
import { DisclosureStateContext, DisclosurePanel, Heading, Disclosure } from 'react-aria-components/Disclosure';
import { DisclosureGroup } from 'react-aria-components/DisclosureGroup';
import { dataAttr } from '../../utils/assertion.js';
import { composeTwRenderProps, composeSlotClassName } from '../../utils/compose.js';
import { dom } from '../../utils/dom.js';
import { IconChevronDown } from '../icons.js';
import { jsx, Fragment } from 'react/jsx-runtime';
import { SurfaceContext } from '../surface/surface.js';

const AccordionContext = /*#__PURE__*/createContext({});

/* -------------------------------------------------------------------------------------------------
 * Accordion Root
 * -----------------------------------------------------------------------------------------------*/

const AccordionRoot = ({
  children,
  className,
  hideSeparator = false,
  variant,
  ...props
}) => {
  const slots = React__default.useMemo(() => accordionVariants({
    variant
  }), [variant]);
  const content = /*#__PURE__*/jsx(DisclosureGroup, {
    className: composeTwRenderProps(className, slots.base()),
    "data-slot": "accordion",
    ...props,
    children: values => /*#__PURE__*/jsx(Fragment, {
      children: typeof children === "function" ? children(values) : children
    })
  });
  return /*#__PURE__*/jsx(AccordionContext, {
    value: {
      slots,
      hideSeparator
    },
    children: variant === "surface" ?
    /*#__PURE__*/
    // Allows inner components to apply "on-surface" colors for proper contrast
    jsx(SurfaceContext, {
      value: {
        variant: "default"
      },
      children: content
    }) : content
  });
};

/* -------------------------------------------------------------------------------------------------
 * AccordionItem
 * -----------------------------------------------------------------------------------------------*/

const AccordionItem = ({
  className,
  ...props
}) => {
  const {
    hideSeparator,
    slots
  } = use(AccordionContext);
  return /*#__PURE__*/jsx(Disclosure, {
    className: composeTwRenderProps(className, slots?.item()),
    "data-hide-separator": hideSeparator ? "true" : undefined,
    "data-slot": "accordion-item",
    ...props,
    children: props.children
  });
};

/* -------------------------------------------------------------------------------------------------
 * AccordionIndicator
 * -----------------------------------------------------------------------------------------------*/

const AccordionIndicator = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(AccordionContext);
  const {
    isExpanded
  } = use(DisclosureStateContext);
  if (children && /*#__PURE__*/React__default.isValidElement(children)) {
    return /*#__PURE__*/React__default.cloneElement(children, {
      ...props,
      "data-expanded": dataAttr(isExpanded),
      className: composeSlotClassName(slots?.indicator, className),
      "data-slot": "accordion-indicator"
    });
  }
  return /*#__PURE__*/jsx(IconChevronDown, {
    className: composeSlotClassName(slots?.indicator, className),
    "data-expanded": dataAttr(isExpanded),
    "data-slot": "accordion-indicator",
    ...props
  });
};

/* -------------------------------------------------------------------------------------------------
 * AccordionHeading
 * -----------------------------------------------------------------------------------------------*/

const AccordionHeading = ({
  className,
  ...props
}) => {
  const {
    slots
  } = use(AccordionContext);
  return /*#__PURE__*/jsx(Heading, {
    className: composeSlotClassName(slots?.heading, className),
    "data-slot": "accordion-heading",
    ...props
  });
};

/* -------------------------------------------------------------------------------------------------
 * AccordionTrigger
 * -----------------------------------------------------------------------------------------------*/

const AccordionTrigger = ({
  className,
  ...props
}) => {
  const {
    slots
  } = use(AccordionContext);
  return /*#__PURE__*/jsx(Button, {
    className: composeTwRenderProps(className, slots?.trigger()),
    "data-slot": "accordion-trigger",
    slot: "trigger",
    ...props,
    children: values => /*#__PURE__*/jsx(Fragment, {
      children: typeof props.children === "function" ? props.children(values) : props.children
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * AccordionBody
 * -----------------------------------------------------------------------------------------------*/

const AccordionBody = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(AccordionContext);
  return /*#__PURE__*/jsx(dom.div, {
    className: slots?.body({}),
    "data-slot": "accordion-body",
    ...props,
    children: /*#__PURE__*/jsx("div", {
      className: composeSlotClassName(slots?.bodyInner, className),
      children: children
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * AccordionPanel
 * -----------------------------------------------------------------------------------------------*/

const AccordionPanel = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(AccordionContext);
  const {
    isExpanded
  } = use(DisclosureStateContext);
  return /*#__PURE__*/jsx(DisclosurePanel, {
    className: composeTwRenderProps(className, slots?.panel()),
    "data-expanded": dataAttr(isExpanded),
    "data-slot": "accordion-panel",
    ...props,
    children: children
  });
};

export { AccordionBody, AccordionHeading, AccordionIndicator, AccordionItem, AccordionPanel, AccordionRoot, AccordionTrigger };
