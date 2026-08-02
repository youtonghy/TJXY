"use client";
import { tagGroupVariants } from '@heroui/styles';
import { useMemo, createContext, use } from 'react';
import { TagGroup, TagList } from 'react-aria-components/TagGroup';
import { composeTwRenderProps } from '../../utils/compose.js';
import { FieldSlotsGate } from '../../utils/field-slots-gate.js';
import { jsx } from 'react/jsx-runtime';

const TagGroupContext = /*#__PURE__*/createContext({});

/* -------------------------------------------------------------------------------------------------
 * TagGroup Root
 * -----------------------------------------------------------------------------------------------*/

const TagGroupRoot = ({
  children,
  className,
  size,
  variant,
  ...restProps
}) => {
  const slots = useMemo(() => tagGroupVariants(), []);
  return /*#__PURE__*/jsx(FieldSlotsGate, {
    children: /*#__PURE__*/jsx(TagGroupContext, {
      value: {
        slots,
        size,
        variant
      },
      children: /*#__PURE__*/jsx(TagGroup, {
        className: slots.base({
          className
        }),
        "data-slot": "tag-group",
        ...restProps,
        children: children
      })
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * TagGroup List
 * -----------------------------------------------------------------------------------------------*/

const TagGroupList = ({
  children,
  className,
  ...restProps
}) => {
  const {
    slots
  } = use(TagGroupContext);
  return /*#__PURE__*/jsx(TagList, {
    className: composeTwRenderProps(className, slots?.list()),
    "data-slot": "tag-group-list",
    ...restProps,
    children: children
  });
};

export { TagGroupContext, TagGroupList, TagGroupRoot };
