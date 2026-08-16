"use client";
import { listboxVariants } from '@heroui/styles';
import React__default from 'react';
import { ListBox } from 'react-aria-components/ListBox';
import { jsx } from 'react/jsx-runtime';
import { composeTwRenderProps } from '../../utils/compose.js';

function ListBoxRoot({
  className,
  variant,
  ...props
}) {
  const styles = React__default.useMemo(() => listboxVariants({
    variant
  }), [variant]);
  return /*#__PURE__*/jsx(ListBox, {
    className: composeTwRenderProps(className, styles),
    "data-slot": "list-box",
    ...props
  });
}

export { ListBoxRoot };
