import { tv } from 'tailwind-variants';

const inputGroupVariants = tv({
    defaultVariants: {
        fullWidth: false,
        variant: "primary",
    },
    slots: {
        base: "input-group",
        input: "input-group__input",
        prefix: "input-group__prefix",
        suffix: "input-group__suffix",
    },
    variants: {
        fullWidth: {
            false: {},
            true: {
                base: "input-group--full-width",
            },
        },
        variant: {
            primary: {
                base: "input-group--primary",
            },
            secondary: {
                base: "input-group--secondary",
            },
        },
    },
});

export { inputGroupVariants };
