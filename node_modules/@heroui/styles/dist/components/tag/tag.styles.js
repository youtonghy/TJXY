import { tv } from 'tailwind-variants';

const tagVariants = tv({
    defaultVariants: {
        size: "md",
        variant: "default",
    },
    slots: {
        base: "tag",
        removeButton: "tag__remove-button",
    },
    variants: {
        size: {
            lg: {
                base: "tag--lg",
            },
            md: {
                base: "tag--md",
            },
            sm: {
                base: "tag--sm",
            },
        },
        variant: {
            default: {
                base: "tag--default",
            },
            surface: {
                base: "tag--surface",
            },
        },
    },
});

export { tagVariants };
