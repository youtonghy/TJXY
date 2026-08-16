"use client";
import { tabsVariants } from '@heroui/styles';
import React__default, { createContext, use, useRef, useCallback } from 'react';
import { SelectionIndicator } from 'react-aria-components/SelectionIndicator';
import { Tabs, TabPanel as TabPanel$1, Tab as Tab$1, TabList as TabList$1 } from 'react-aria-components/Tabs';
import { composeTwRenderProps, composeSlotClassName } from '../../utils/compose.js';
import { dom } from '../../utils/dom.js';
import { IconChevronUp, IconChevronLeft, IconChevronDown, IconChevronRight } from '../icons.js';
import { ScrollShadow } from '../scroll-shadow/index.js';
import { jsx, jsxs } from 'react/jsx-runtime';
import { createCollectionSlot } from '../../utils/collection-prop-injection.js';

const TabsContext = /*#__PURE__*/createContext({});

/* -------------------------------------------------------------------------------------------------
 * Tabs Collection Slot
 * -----------------------------------------------------------------------------------------------*/

const listContainerSlot = createCollectionSlot("tabs.listContainer");

/* -------------------------------------------------------------------------------------------------
 * Tabs Root
 * -----------------------------------------------------------------------------------------------*/

const TabsRoot = ({
  children,
  className,
  orientation = "horizontal",
  variant,
  ...props
}) => {
  const slots = React__default.useMemo(() => tabsVariants({
    variant
  }), [variant]);
  return /*#__PURE__*/jsx(TabsContext, {
    value: {
      orientation,
      slots
    },
    children: /*#__PURE__*/jsx(Tabs, {
      ...props,
      className: composeTwRenderProps(className, slots.base()),
      "data-slot": "tabs",
      orientation: orientation,
      children: children
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * Tabs List Container
 * -----------------------------------------------------------------------------------------------*/

const TabListContainer = ({
  children,
  className,
  render,
  ...containerProps
}) => {
  return /*#__PURE__*/jsx(listContainerSlot.Injector, {
    ...containerProps,
    className,
    render,
    children: children
  });
};

/* -------------------------------------------------------------------------------------------------
 * Tabs List
 * -----------------------------------------------------------------------------------------------*/

const TabList = ({
  children,
  className,
  ...props
}) => {
  const {
    orientation = "horizontal",
    slots
  } = use(TabsContext);
  const scrollerRef = useRef(null);
  const isVertical = orientation === "vertical";
  const [listContainerProps, restProps] = listContainerSlot.useSlot(props);
  const scrollBy = useCallback(direction => {
    const el = scrollerRef.current;
    if (!el) return;
    const size = isVertical ? el.clientHeight : el.clientWidth;
    const scrollSize = isVertical ? el.scrollHeight : el.scrollWidth;
    const maxScroll = Math.max(0, scrollSize - size);

    // In RTL, the horizontal scroll range runs from 0 (start, on the right) to negative,
    // so the delta sign must be flipped for `scrollLeft` to move toward the intended edge.
    const isRTL = !isVertical && getComputedStyle(el).direction === "rtl";
    const delta = direction * size * 0.8 * (isRTL ? -1 : 1);
    const current = isVertical ? el.scrollTop : el.scrollLeft;

    // Clamp the target to the scrollable range so a press near the edge lands
    // flush on it instead of requesting a position past the content, which can
    // cut the smooth scroll short of the edge (e.g. iOS Safari) or leave the
    // strip visually stranded. RTL ranges run [-maxScroll, 0].
    const next = Math.min(isRTL ? 0 : maxScroll, Math.max(isRTL ? -maxScroll : 0, current + delta));
    if (next === current) return;
    el.scrollTo({
      behavior: "smooth",
      [isVertical ? "top" : "left"]: next
    });
  }, [isVertical]);

  // Without ListContainer, stay a thin RAC TabList
  if (!listContainerProps) {
    return /*#__PURE__*/jsx(TabList$1, {
      ...restProps,
      className: composeTwRenderProps(className, slots?.tabList()),
      "data-slot": "tabs-list",
      children: children
    });
  }
  const {
    className: containerClassName,
    render: containerRender,
    ...containerRest
  } = listContainerProps;
  return /*#__PURE__*/jsx(TabList$1, {
    ...restProps,
    className: composeTwRenderProps(className, slots?.tabList()),
    "data-slot": "tabs-list",
    render: renderProps => {
      const {
        children: listChildren,
        className: listClassName,
        ref: listRef,
        ...listRest
      } = renderProps;
      return /*#__PURE__*/jsxs(dom.div, {
        className: composeSlotClassName(slots?.tabListContainer, containerClassName),
        "data-slot": "tabs-list-container",
        render: containerRender,
        ...containerRest,
        children: [/*#__PURE__*/jsx(ScrollShadow, {
          ref: scrollerRef,
          hideScrollBar: true,
          className: composeSlotClassName(slots?.scroller),
          orientation: orientation,
          size: 64,
          children: /*#__PURE__*/jsx("div", {
            ...listRest,
            ref: listRef,
            className: listClassName,
            children: listChildren
          })
        }), /*#__PURE__*/jsx("button", {
          "aria-label": isVertical ? "Scroll tabs up" : "Scroll tabs left",
          className: composeSlotClassName(slots?.scrollPrev),
          tabIndex: -1,
          type: "button",
          onClick: () => scrollBy(-1),
          children: isVertical ? /*#__PURE__*/jsx(IconChevronUp, {}) : /*#__PURE__*/jsx(IconChevronLeft, {})
        }), /*#__PURE__*/jsx("button", {
          "aria-label": isVertical ? "Scroll tabs down" : "Scroll tabs right",
          className: composeSlotClassName(slots?.scrollNext),
          tabIndex: -1,
          type: "button",
          onClick: () => scrollBy(1),
          children: isVertical ? /*#__PURE__*/jsx(IconChevronDown, {}) : /*#__PURE__*/jsx(IconChevronRight, {})
        })]
      });
    },
    children: children
  });
};

/* -------------------------------------------------------------------------------------------------
 * Tab
 * -----------------------------------------------------------------------------------------------*/

const Tab = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(TabsContext);
  return /*#__PURE__*/jsx(Tab$1, {
    ...props,
    className: composeTwRenderProps(className, slots?.tab()),
    "data-slot": "tabs-tab",
    children: children
  });
};

/* -------------------------------------------------------------------------------------------------
 * Tab Indicator
 * -----------------------------------------------------------------------------------------------*/

const TabIndicator = ({
  className,
  ...props
}) => {
  const {
    slots
  } = use(TabsContext);
  return /*#__PURE__*/jsx(SelectionIndicator, {
    className: composeSlotClassName(slots?.tabIndicator, className),
    "data-slot": "tabs-indicator",
    ...props
  });
};

/* -------------------------------------------------------------------------------------------------
 * Tab Panel
 * -----------------------------------------------------------------------------------------------*/

const TabPanel = ({
  children,
  className,
  ...props
}) => {
  const {
    slots
  } = use(TabsContext);
  return /*#__PURE__*/jsx(TabPanel$1, {
    ...props,
    className: composeTwRenderProps(className, slots?.tabPanel()),
    "data-slot": "tabs-panel",
    children: children
  });
};

/* -------------------------------------------------------------------------------------------------
 * Tab Separator
 * -----------------------------------------------------------------------------------------------*/

const TabSeparator = ({
  className,
  ...props
}) => {
  const {
    slots
  } = use(TabsContext);
  return /*#__PURE__*/jsx(dom.span, {
    "aria-hidden": "true",
    className: composeSlotClassName(slots?.separator, className),
    "data-slot": "tabs-separator",
    ...props
  });
};

export { Tab, TabIndicator, TabList, TabListContainer, TabPanel, TabSeparator, TabsRoot };
