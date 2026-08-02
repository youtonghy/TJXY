import React__default, { createContext, forwardRef, useContext, Children, isValidElement } from 'react';
import { Logger } from './logger.js';
import { jsx } from 'react/jsx-runtime';

const logger = new Logger({
  prefix: "HeroUI"
});

/** Dev-only registry for duplicate slot name warnings. */
let registry;
const __DEV__ = typeof process !== "undefined" && process.env?.["NODE_ENV"] !== "production";

/**
 * Create a CollectionBuilder-safe slot for optional chrome around a RAC collection
 * target (e.g. TabList, ListBox).
 *
 * The injector emits no host DOM: chrome props are provided via Context and a
 * string-keyed `cloneElement` channel (React strips Symbol keys from element props).
 * The target should apply chrome through the RAC `render` prop — not by wrapping
 * the collection child in a host node.
 */
const createCollectionSlot = name => {
  if (__DEV__) {
    registry ??= new Set();
    if (registry.has(name)) {
      logger.warn(`Duplicate collection slot "${name}". Use a unique, namespaced name (e.g. "tabs.listContainer", "menu.popover").`);
    }
    registry.add(name);
  }

  // String key required — React drops Symbol keys from element props / cloneElement.
  const key = `$$heroui.collection.${name}`;
  const SlotContext = /*#__PURE__*/createContext(undefined);
  const inject = (children, value) => {
    return Children.map(children, child => {
      if (! /*#__PURE__*/isValidElement(child)) {
        return child;
      }
      return /*#__PURE__*/React__default.cloneElement(child, {
        [key]: value
      });
    });
  };
  const consume = props => {
    const {
      [key]: injected,
      ...rest
    } = props;
    return [injected, rest];
  };
  const useSlot = props => {
    const [fromProps, rest] = consume(props);
    const fromContext = useContext(SlotContext);
    return [fromProps ?? fromContext, rest];
  };
  const Injector = ({
    children,
    ...value
  }) => {
    return /*#__PURE__*/jsx(SlotContext.Provider, {
      value: value,
      children: inject(children, value)
    });
  };
  Injector.displayName = `HeroUI.CollectionSlot(${name})`;
  const withSlot = (Primitive, defaultRender) => {
    const Wrapped = /*#__PURE__*/forwardRef(function CollectionSlotTarget(props, forwardedRef) {
      const [injected, rest] = useSlot(props);
      const {
        children,
        ...restProps
      } = rest;
      if (!injected) {
        return /*#__PURE__*/jsx(Primitive, {
          ...restProps,
          ref: forwardedRef,
          children: children
        });
      }
      if (defaultRender) {
        // eslint-disable-next-line react-hooks/refs -- forward ref into chrome factory; `.current` is not read here
        return defaultRender({
          ...restProps,
          children
        }, injected, forwardedRef);
      }
      const {
        className: containerClassName,
        render: customRender,
        ...containerRest
      } = injected;
      return /*#__PURE__*/jsx(Primitive, {
        ...restProps,
        ref: forwardedRef,
        children: customRender ? customRender({
          children,
          className: containerClassName,
          ...containerRest
        }) : /*#__PURE__*/jsx("div", {
          className: containerClassName,
          "data-slot": `${name}-container`,
          ...containerRest,
          children: children
        })
      });
    });
    Wrapped.displayName = `HeroUI.withCollectionSlot(${Primitive.displayName || Primitive.name || name})`;
    return Wrapped;
  };
  return {
    key,
    inject,
    consume,
    useSlot,
    Injector,
    withSlot,
    Context: SlotContext
  };
};

export { createCollectionSlot };
