import { Children, isValidElement } from 'react';

const pickChildren = (children, targetChild) => {
  const target = [];
  const withoutTargetChildren = Children.map(children, item => {
    if (! /*#__PURE__*/isValidElement(item)) return item;
    if (item.type === targetChild) {
      target.push(item);
      return null;
    }
    return item;
  })?.filter(Boolean);
  const targetChildren = target.length >= 0 ? target : undefined;
  return [withoutTargetChildren, targetChildren];
};

export { pickChildren };
