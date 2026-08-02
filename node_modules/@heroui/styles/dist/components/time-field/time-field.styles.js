import { tv } from 'tailwind-variants';

const timeFieldVariants = tv({
    base: "time-field",
    defaultVariants: {
        fullWidth: false,
    },
    variants: {
        fullWidth: {
            false: "",
            true: "time-field--full-width",
        },
    },
});

export { timeFieldVariants };
