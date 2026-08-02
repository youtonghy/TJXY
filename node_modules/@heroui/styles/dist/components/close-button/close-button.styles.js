import { tv } from 'tailwind-variants';

const closeButtonVariants = tv({
    base: "close-button",
    defaultVariants: {
        variant: "default",
    },
    variants: {
        variant: {
            default: "close-button--default",
        },
    },
});

export { closeButtonVariants };
