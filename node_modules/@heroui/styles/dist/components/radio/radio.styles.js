import { tv } from 'tailwind-variants';

const radioVariants = tv({
    slots: {
        base: "radio",
        content: "radio__content",
        control: "radio__control",
        indicator: "radio__indicator",
    },
});

export { radioVariants };
