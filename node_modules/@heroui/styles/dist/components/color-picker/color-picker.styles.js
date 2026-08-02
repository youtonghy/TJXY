import { tv } from 'tailwind-variants';

const colorPickerVariants = tv({
    slots: {
        base: "color-picker",
        popover: "color-picker__popover",
        trigger: "color-picker__trigger",
    },
});

export { colorPickerVariants };
