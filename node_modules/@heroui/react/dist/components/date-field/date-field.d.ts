import type { DateFieldVariants } from "@heroui/styles";
import type { ComponentPropsWithRef } from "react";
import type { DateValue } from "react-aria-components/Calendar";
import { DateField as DateFieldPrimitive } from "react-aria-components/DateField";
interface DateFieldRootProps<T extends DateValue> extends ComponentPropsWithRef<typeof DateFieldPrimitive<T>>, DateFieldVariants {
}
declare function DateFieldRoot<T extends DateValue>({ children, className, fullWidth, ...props }: DateFieldRootProps<T>): import("react/jsx-runtime").JSX.Element;
export { DateFieldRoot };
export type { DateFieldRootProps };
