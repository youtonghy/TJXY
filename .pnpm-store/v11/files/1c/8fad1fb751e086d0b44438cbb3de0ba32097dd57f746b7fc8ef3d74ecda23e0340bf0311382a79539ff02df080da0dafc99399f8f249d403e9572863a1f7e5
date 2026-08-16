"use client";
import { scrollShadowVariants } from '@heroui/styles';
import { mergeRefs } from '@react-aria/utils';
import { useRef, useMemo } from 'react';
import { useSafeLayoutEffect } from '../../hooks/use-safe-layout-effect.js';
import { useScrollShadow } from './use-scroll-shadow.js';
import { jsx } from 'react/jsx-runtime';

const ScrollShadowRoot = ({
  children,
  className,
  hideScrollBar = false,
  isEnabled = true,
  offset = 0,
  onVisibilityChange,
  orientation = "vertical",
  ref,
  size = 40,
  variant = "fade",
  visibility = "auto",
  ...props
}) => {
  const internalRef = useRef(null);
  useScrollShadow({
    containerRef: internalRef,
    isEnabled,
    offset,
    onVisibilityChange,
    orientation,
    visibility
  });

  // Handle controlled visibility mode
  useSafeLayoutEffect(() => {
    const el = internalRef.current;
    if (!el || visibility === "auto") return;

    // Clear all data attributes
    delete el.dataset["topScroll"];
    delete el.dataset["bottomScroll"];
    delete el.dataset["topBottomScroll"];
    delete el.dataset["leftScroll"];
    delete el.dataset["rightScroll"];
    delete el.dataset["leftRightScroll"];

    // Set controlled visibility
    if (visibility === "both") {
      el.dataset[orientation === "vertical" ? "topBottomScroll" : "leftRightScroll"] = "true";
    } else if (visibility !== "none") {
      el.dataset[`${visibility}Scroll`] = "true";
    }
  }, [visibility, orientation]);
  const slots = useMemo(() => scrollShadowVariants({
    hideScrollBar,
    orientation,
    variant
  }), [orientation, hideScrollBar, variant]);
  const style = {
    "--scroll-shadow-size": `${size}px`,
    ...props.style
  };
  return /*#__PURE__*/jsx("div", {
    ref: mergeRefs(internalRef, ref),
    className: slots.base({
      className
    }),
    "data-orientation": orientation,
    "data-scroll-shadow-size": size,
    "data-slot": "scroll-shadow",
    style: style,
    ...props,
    children: children
  });
};
ScrollShadowRoot.displayName = "HeroUI.ScrollShadow";

export { ScrollShadowRoot };
