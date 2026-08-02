"use client";
import { fieldsetVariants } from '@heroui/styles';
import React__default, { createContext, use } from 'react';
import { Provider, ButtonContext, CheckboxGroupContext, LinkContext, RadioGroupContext, SliderContext, ToggleButtonContext, ToggleButtonGroupContext } from 'react-aria-components';
import { composeSlotClassName } from '../../utils/compose.js';
import { dom } from '../../utils/dom.js';
import { jsx } from 'react/jsx-runtime';

const FieldsetContext = /*#__PURE__*/createContext({});

/* -------------------------------------------------------------------------------------------------
 * Fieldset Root
 * -----------------------------------------------------------------------------------------------*/

const FieldsetRoot = ({
  children,
  className,
  ...props
}) => {
  const slots = React__default.useMemo(() => fieldsetVariants({}), []);

  // Mirror native `<fieldset disabled>` as `data-disabled="true"` so the
  // existing `[data-disabled="true"] .label` (and similar) ancestor selectors
  // cascade disabled styling to descendant fields, just like a direct
  // `isDisabled` prop on TextField/Checkbox/etc. would.
  const isDisabled = "disabled" in props && props.disabled === true;
  return /*#__PURE__*/jsx(FieldsetContext, {
    value: {
      slots
    },
    children: /*#__PURE__*/jsx(dom.fieldset, {
      className: slots?.base({
        className
      }),
      "data-disabled": isDisabled || undefined,
      "data-slot": "fieldset",
      ...props,
      children: isDisabled ?
      /*#__PURE__*/
      // Forward `isDisabled` through React Aria contexts so descendant
      // components stay consistent with the native `<fieldset disabled>`
      // behaviour. There are two reasons we need to do this manually:
      //
      // 1. Some primitives (Slider, RadioGroup, CheckboxGroup, ...) render
      //    as `<div>` so the browser does not propagate the fieldset's
      //    `disabled` attribute to them — without this, they would still
      //    look enabled and remain interactive.
      // 2. Other primitives (Button, ToggleButton, Link, ...) do get
      //    natively disabled by the browser, but React Aria's internal
      //    `isDisabled` state — which drives `data-disabled` and the
      //    `{isDisabled}` render prop — only updates from props/context.
      //    Without this, the button is unclickable but its render prop
      //    keeps returning `isDisabled: false`.
      jsx(Provider, {
        values: [[ButtonContext, {
          isDisabled: true
        }], [CheckboxGroupContext, {
          isDisabled: true
        }], [LinkContext, {
          isDisabled: true
        }], [RadioGroupContext, {
          isDisabled: true
        }], [SliderContext, {
          isDisabled: true
        }], [ToggleButtonContext, {
          isDisabled: true
        }], [ToggleButtonGroupContext, {
          isDisabled: true
        }]],
        children: children
      }) : children
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * Fieldset Legend
 * -----------------------------------------------------------------------------------------------*/

const FieldsetLegend = ({
  className,
  ...props
}) => {
  const {
    slots
  } = use(FieldsetContext);
  return /*#__PURE__*/jsx(dom.legend, {
    className: composeSlotClassName(slots?.legend, className),
    "data-slot": "fieldset-legend",
    ...props
  });
};

/* -------------------------------------------------------------------------------------------------
 * Field Group
 * -----------------------------------------------------------------------------------------------*/

const FieldGroup = ({
  className,
  ...rest
}) => {
  const {
    slots
  } = use(FieldsetContext);
  return /*#__PURE__*/jsx(dom.div, {
    className: composeSlotClassName(slots?.fieldGroup, className),
    "data-slot": "fieldset-field-group",
    ...rest
  });
};

/* -------------------------------------------------------------------------------------------------
 * Field Actions
 * -----------------------------------------------------------------------------------------------*/

const FieldsetActions = ({
  children,
  className,
  ...rest
}) => {
  const {
    slots
  } = use(FieldsetContext);
  return /*#__PURE__*/jsx(dom.div, {
    className: composeSlotClassName(slots?.actions, className),
    "data-slot": "fieldset-actions",
    ...rest,
    children: children
  });
};

export { FieldGroup, FieldsetActions, FieldsetLegend, FieldsetRoot };
