import { tv } from 'tailwind-variants';

const textAreaVariants = tv({
    base: "textarea",
    defaultVariants: {
        fullWidth: false,
        variant: "primary",
    },
    variants: {
        fullWidth: {
            false: "",
            true: "textarea--full-width",
        },
        variant: {
            primary: "textarea--primary",
            secondary: "textarea--secondary",
        },
    },
});

export { textAreaVariants };
