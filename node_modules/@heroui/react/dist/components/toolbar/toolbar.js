"use client";
import { toolbarVariants } from '@heroui/styles';
import React__default from 'react';
import { SeparatorContext } from 'react-aria-components/Separator';
import { ToggleButtonGroupContext } from 'react-aria-components/ToggleButtonGroup';
import { Toolbar } from 'react-aria-components/Toolbar';
import { jsx } from 'react/jsx-runtime';
import { composeTwRenderProps } from '../../utils/compose.js';

const ToolbarRoot = ({
  children,
  className,
  isAttached,
  orientation = "horizontal",
  ...props
}) => {
  const styles = React__default.useMemo(() => toolbarVariants({
    isAttached,
    orientation
  }), [isAttached, orientation]);
  return /*#__PURE__*/jsx(ToggleButtonGroupContext.Provider, {
    value: {
      orientation
    },
    children: /*#__PURE__*/jsx(SeparatorContext.Provider, {
      value: {
        orientation: orientation === "horizontal" ? "vertical" : "horizontal"
      },
      children: /*#__PURE__*/jsx(Toolbar, {
        className: composeTwRenderProps(className, styles),
        "data-slot": "toolbar",
        orientation: orientation,
        ...props,
        children: children
      })
    })
  });
};
ToolbarRoot.displayName = "HeroUI.Toolbar";

export { ToolbarRoot };
