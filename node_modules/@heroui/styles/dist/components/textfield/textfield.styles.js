import { tv } from 'tailwind-variants';

const textFieldVariants = tv({
    base: "textfield",
    defaultVariants: {
        fullWidth: false,
    },
    variants: {
        fullWidth: {
            false: "",
            true: "textfield--full-width",
        },
    },
});

export { textFieldVariants };
