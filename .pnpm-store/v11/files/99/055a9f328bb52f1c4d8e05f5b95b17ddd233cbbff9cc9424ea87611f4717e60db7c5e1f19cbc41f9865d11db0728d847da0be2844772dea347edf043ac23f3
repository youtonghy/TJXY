import { mergeRefs, useLayoutEffect } from '@react-aria/utils';
import { useRef, useMemo } from 'react';
import { jsx } from 'react/jsx-runtime';

// eslint-disable-next-line react-refresh/only-export-components
function DOMElement(ElementType, props) {
  const {
    ref: forwardedRef,
    render,
    ...otherProps
  } = props;
  const elementRef = useRef(null);
  const ref = useMemo(() => mergeRefs(forwardedRef, elementRef), [forwardedRef, elementRef]);
  useLayoutEffect(() => {
    if (typeof process !== "undefined" && process.env?.["NODE_ENV"] !== "production" && render) {
      if (!elementRef.current) {
        console.warn("Ref was not connected to DOM element returned by custom `render` function. Did you forget to pass through or merge the `ref`?");
      }
    }
  }, [ElementType, render]);
  const domProps = {
    ...otherProps,
    ref
  };
  if (render) {
    return render(domProps, undefined);
  }
  return /*#__PURE__*/jsx(ElementType, {
    ...domProps
  });
}
const domComponentCache = {};

// Dynamically generates and caches components for each DOM element (e.g. `dom.button`).
const dom = new Proxy({}, {
  get(_target, elementType) {
    if (typeof elementType !== "string") {
      return undefined;
    }
    let res = domComponentCache[elementType];
    if (!res) {
      res = DOMElement.bind(null, elementType);
      domComponentCache[elementType] = res;
    }
    return res;
  }
});

export { dom };
