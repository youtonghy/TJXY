"use client";
import { tagVariants } from '@heroui/styles';
import { use, useMemo, Children, createContext } from 'react';
import { Tag } from 'react-aria-components/TagGroup';
import { pickChildren } from '../../utils/children.js';
import { composeTwRenderProps } from '../../utils/compose.js';
import { CloseButton } from '../close-button/index.js';
import '../tag-group/index.js';
import { jsx, jsxs, Fragment } from 'react/jsx-runtime';
import { TagGroupContext } from '../tag-group/tag-group.js';

const TagContext = /*#__PURE__*/createContext({});

/* -------------------------------------------------------------------------------------------------
 * Tag Root
 * -----------------------------------------------------------------------------------------------*/

const TagRoot = ({
  children,
  className,
  ...restProps
}) => {
  const {
    size,
    variant
  } = use(TagGroupContext);
  const slots = useMemo(() => tagVariants({
    size,
    variant
  }), [size, variant]);
  const textValue = useMemo(() => {
    if (typeof children === "string") {
      return children;
    }
    if (typeof children === "object") {
      return Children.toArray(children).filter(node => typeof node === "string").at(0);
    }
    return undefined;
  }, [children]);

  // Extract custom RemoveButton from children if present
  const [childrenWithoutRemoveButton, removeButtonChildren] = useMemo(() => {
    if (typeof children === "function") {
      return [children, undefined];
    }
    return pickChildren(children, TagRemoveButton);
  }, [children]);
  return /*#__PURE__*/jsx(Tag, {
    className: composeTwRenderProps(className, slots.base()),
    "data-slot": "tag",
    textValue: textValue,
    ...restProps,
    children: renderProps => /*#__PURE__*/jsx(TagContext, {
      value: {
        slots
      },
      children: typeof children === "function" ? children(renderProps) : /*#__PURE__*/jsxs(Fragment, {
        children: [childrenWithoutRemoveButton, !!renderProps.allowsRemoving && (removeButtonChildren && removeButtonChildren.length > 0 ? removeButtonChildren : /*#__PURE__*/jsx(TagRemoveButton, {}))]
      })
    })
  });
};

/* -------------------------------------------------------------------------------------------------
 * Tag Remove Button
 * -----------------------------------------------------------------------------------------------*/

const TagRemoveButton = ({
  children,
  className,
  ...restProps
}) => {
  const {
    slots
  } = use(TagContext);
  return /*#__PURE__*/jsx(CloseButton, {
    "aria-label": "Remove tag",
    className: composeTwRenderProps(className, slots?.removeButton()),
    "data-slot": "tag-remove-button",
    slot: "remove",
    ...restProps,
    children: children
  });
};

export { TagRemoveButton, TagRoot };
