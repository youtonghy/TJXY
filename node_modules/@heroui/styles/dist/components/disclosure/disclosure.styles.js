import { tv } from 'tailwind-variants';

const disclosureVariants = tv({
    defaultVariants: {},
    slots: {
        base: "disclosure",
        body: "disclosure__body",
        bodyInner: "disclosure__body-inner",
        content: "disclosure__content",
        heading: "disclosure__heading",
        indicator: "disclosure__indicator",
        trigger: "disclosure__trigger",
    },
    variants: {},
});

export { disclosureVariants };
