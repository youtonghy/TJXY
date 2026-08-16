"use client";
import { autocompleteVariants } from '@heroui/styles';
import { mergeRefs, useResizeObserver } from '@react-aria/utils';
import React__default, { useRef, createContext, use, useState, useCallback } from 'react';
import { useIsHidden } from 'react-aria/private/collections/Hidden';
import { Autocomplete } from 'react-aria-components/Autocomplete';
import { Button } from 'react-aria-components/Button';
import { Group } from 'react-aria-components/Group';
import { Popover } from 'react-aria-components/Popover';
import { Select, SelectStateContext, SelectValue } from 'react-aria-components/Select';
import { dataAttr } from '../../utils/assertion.js';
import { composeTwRenderProps, composeSlotClassName } from '../../utils/compose.js';
import { dom } from '../../utils/dom.js';
import { FieldSlotsGate } from '../../utils/field-slots-gate.js';
import { CloseIcon, IconChevronDown } from '../icons.js';
import { jsx, Fragment } from 'react/jsx-runtime';
import { SurfaceContext } from '../surface/surface.js';

const AutocompleteContext = /*#__PURE__*/createContext({
  triggerRef: {
    current: null
  },
  clearButtonRef: {
    current: null
  },
  isDisabled: false
});

/* -------------------------------------------------------------------------------------------------
 * Autocomplete Root
 * -----------------------------------------------------------------------------------------------*/

const AutocompleteRoot = ({
  children,
  className,
  fullWidth,
  isDisabled,
  onClear,
  variant,
  ...props
}) => {
  const slots = React__default.useMemo(() => autocompleteVariants({
    fullWidth,
    variant
  }), [fullWidth, variant]);
  const triggerRef = useRef(null);
  const clearButtonRef = useRef(null);
  return /*#__PURE__*/jsx(FieldSlotsGate, {
    children: /*#__PURE__*/jsx(AutocompleteContext, {
      value: {
        slots,
        triggerRef,
        clearButtonRef,
        onClear,
        isDisabled
      },
      children: /*#__PURE__*/jsx(Select, {
        "data-slot": "autocomplete",
        ...props,
        className: composeTwRenderProps(className, slots?.base()),
        isDisabled: isDisabled,
        children: values => /*#__PURE__*/jsx(Fragment, {
          children: typeof children === "function" ? children(values) : children
        })
      })
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * Autocomplete Trigger
 * -----------------------------------------------------------------------------------------------*/

const AutocompleteTrigger = ({
  children,
  className,
  isDisabled: isDisabledProp,
  onClick,
  ref,
  ...props
}) => {
  const {
    clearButtonRef,
    isDisabled: rootDisabled,
    slots,
    triggerRef
  } = use(AutocompleteContext);
  const state = use(SelectStateContext);
  const isDisabled = isDisabledProp ?? rootDisabled ?? false;
  const isHidden = useIsHidden();

  // Callback ref to update context ref
  const contextRefCallback = React__default.useCallback(node => {
    triggerRef.current = node;
  }, [triggerRef]);

  // Merge context ref callback with user-provided ref
  const mergedRef = mergeRefs(contextRefCallback, ref);
  const handleClick = e => {
    // Don't toggle if clicking the clear button
    if (clearButtonRef.current?.contains(e.target)) {
      return;
    }
    onClick?.(e);
    state?.toggle();
  };
  if (isHidden) {
    return null;
  }
  return /*#__PURE__*/jsx(Group, {
    ref: mergedRef,
    className: composeTwRenderProps(className, slots?.trigger()),
    "data-slot": "autocomplete-trigger",
    isDisabled: isDisabled,
    onClick: handleClick,
    ...props,
    children: values => /*#__PURE__*/jsx(Fragment, {
      children: typeof children === "function" ? children(values) : children
    })
  });
};
AutocompleteTrigger.displayName = "AutocompleteTrigger";

/* -------------------------------------------------------------------------------------------------
 * Autocomplete Value
 * -----------------------------------------------------------------------------------------------*/

const AutocompleteValue = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(AutocompleteContext);
  return /*#__PURE__*/jsx(SelectValue, {
    className: composeTwRenderProps(className, slots?.value()),
    "data-slot": "autocomplete-value",
    ...props,
    children: children
  });
};

/* -------------------------------------------------------------------------------------------------
 * Autocomplete Indicator
 * -----------------------------------------------------------------------------------------------*/

const AutocompleteIndicator = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(AutocompleteContext);
  const state = use(SelectStateContext);
  const indicator = children && /*#__PURE__*/React__default.isValidElement(children) ? (/*#__PURE__*/React__default.cloneElement(children, {
    ...props,
    className: composeSlotClassName(slots?.indicator, className),
    "data-slot": "autocomplete-indicator",
    "data-open": dataAttr(state?.isOpen)
  })) : /*#__PURE__*/jsx(IconChevronDown, {
    className: composeSlotClassName(slots?.indicator, className),
    "data-open": dataAttr(state?.isOpen),
    "data-slot": "autocomplete-default-indicator",
    ...props
  });
  return /*#__PURE__*/jsx(Button, {
    children: indicator
  });
};

/* -------------------------------------------------------------------------------------------------
 * Autocomplete Popover
 * -----------------------------------------------------------------------------------------------*/

const AutocompletePopover = ({
  children,
  className,
  placement = "bottom",
  style,
  ...props
}) => {
  const {
    slots,
    triggerRef
  } = use(AutocompleteContext);
  const [triggerWidth, setTriggerWidth] = useState(null);
  const onResize = useCallback(() => {
    if (triggerRef.current) {
      setTriggerWidth(triggerRef.current.offsetWidth + "px");
    }
  }, [triggerRef]);
  useResizeObserver({
    ref: triggerRef,
    onResize
  });
  return /*#__PURE__*/jsx(SurfaceContext, {
    value: {
      variant: "default"
    },
    children: /*#__PURE__*/jsx(Popover, {
      ...props,
      className: composeTwRenderProps(className, slots?.popover()),
      "data-slot": "autocomplete-popover",
      placement: placement,
      triggerRef: triggerRef,
      style: {
        "--trigger-width": triggerWidth,
        ...(typeof style === "object" && style !== null ? style : {})
      },
      children: children
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * Autocomplete Filter
 * -----------------------------------------------------------------------------------------------*/

const AutocompleteFilter = ({
  children,
  ...props
}) => {
  return /*#__PURE__*/jsx(Autocomplete, {
    "data-slot": "autocomplete-filter",
    ...props,
    children: children
  });
};

/* -------------------------------------------------------------------------------------------------
 * Autocomplete Clear Button
 * -----------------------------------------------------------------------------------------------*/

const AutocompleteClearButton = ({
  className,
  onClick,
  ref,
  ...props
}) => {
  const {
    clearButtonRef,
    isDisabled,
    onClear,
    slots
  } = use(AutocompleteContext);
  const state = use(SelectStateContext);
  const clearButtonRefCallback = React__default.useCallback(node => {
    clearButtonRef.current = node;
  }, [clearButtonRef]);

  // Merge context ref callback with user-provided ref
  const mergedRef = mergeRefs(clearButtonRefCallback, ref);
  const handleClick = e => {
    state?.selectionManager.setSelectedKeys(new Set());
    onClear?.();
    onClick?.(e);
  };
  const isEmpty = state?.selectionManager.selectedKeys.size === 0;
  return /*#__PURE__*/jsx(dom.button, {
    ref: mergedRef,
    "aria-hidden": isEmpty || undefined,
    "aria-label": "Clear selection",
    className: slots?.clearButton({
      className
    }),
    "data-empty": dataAttr(isEmpty),
    "data-slot": "autocomplete-clear-button",
    disabled: isDisabled ?? false,
    tabIndex: -1,
    type: "button",
    onClick: handleClick,
    ...props,
    children: /*#__PURE__*/jsx(CloseIcon, {
      "data-slot": "autocomplete-clear-button-icon"
    })
  });
};

export { AutocompleteClearButton, AutocompleteFilter, AutocompleteIndicator, AutocompletePopover, AutocompleteRoot, AutocompleteTrigger, AutocompleteValue };
