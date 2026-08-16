import { composeRenderProps } from 'react-aria-components/composeRenderProps';
import { cx } from 'tailwind-variants';

function composeTwRenderProps(className, tailwind) {
  return composeRenderProps(className, (className, renderProps) => {
    const tw = typeof tailwind === "function" ? tailwind(renderProps) ?? "" : tailwind ?? "";
    const cls = className ?? "";
    return cx(tw, cls) ?? "";
  });
}
const composeSlotClassName = (slotFn, className, variants) => {
  return typeof slotFn === "function" ? slotFn({
    ...(variants ?? {}),
    className
  }) : className;
};

export { composeSlotClassName, composeTwRenderProps };
