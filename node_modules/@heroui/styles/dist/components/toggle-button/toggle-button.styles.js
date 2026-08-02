import { tv } from 'tailwind-variants';

const toggleButtonVariants = tv({
    base: "toggle-button",
    defaultVariants: {
        isIconOnly: false,
        size: "md",
        variant: "default",
    },
    variants: {
        isIconOnly: {
            true: "toggle-button--icon-only",
        },
        size: {
            lg: "toggle-button--lg",
            md: "toggle-button--md",
            sm: "toggle-button--sm",
        },
        variant: {
            default: "toggle-button--default",
            ghost: "toggle-button--ghost",
        },
    },
});

export { toggleButtonVariants };
