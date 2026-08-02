"use strict";
import { useState, useCallback, useEffect } from 'react';

const useMeasuredHeight = ref => {
  const [height, setHeight] = useState(undefined);
  const calculateHeight = useCallback(() => {
    if (ref.current) {
      const measuredHeight = ref.current.scrollHeight;
      setHeight(prevHeight => {
        // Only update if height actually changed
        return prevHeight !== measuredHeight ? measuredHeight : prevHeight;
      });
    }
  }, [ref]);
  useEffect(() => {
    const element = ref.current;
    if (!element) {
      return;
    }

    // Use ResizeObserver's initial notification instead of manual call
    const resizeObserver = new ResizeObserver(calculateHeight);

    // Optimize MutationObserver callback
    const mutationObserver = new MutationObserver(mutations => {
      // Check if any mutation is aria-hidden before calculating
      const hasAriaHiddenChange = mutations.some(mutation => mutation.type === "attributes" && mutation.attributeName === "aria-hidden");
      if (hasAriaHiddenChange) {
        calculateHeight();
      }
    });
    resizeObserver.observe(element);
    mutationObserver.observe(element, {
      attributeFilter: ["aria-hidden"],
      attributes: true
    });
    return () => {
      resizeObserver.disconnect();
      mutationObserver.disconnect();
    };
  }, [ref, calculateHeight]);
  return {
    height
  };
};

export { useMeasuredHeight };
