import { tv } from 'tailwind-variants';

const radioGroupVariants = tv({
    base: "radio-group",
    defaultVariants: {
        variant: "primary",
    },
    variants: {
        variant: {
            primary: "radio-group--primary",
            secondary: "radio-group--secondary",
        },
    },
});

export { radioGroupVariants };
