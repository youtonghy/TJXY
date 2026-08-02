import { tv } from 'tailwind-variants';

const fieldsetVariants = tv({
    slots: {
        actions: "fieldset__actions",
        base: "fieldset",
        description: "fieldset__description",
        fieldGroup: "fieldset__field_group",
        legend: "fieldset__legend",
    },
});

export { fieldsetVariants };
