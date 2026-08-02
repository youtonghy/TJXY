"use strict";
import { AlertDialogRoot, AlertDialogCloseTrigger, AlertDialogIcon, AlertDialogFooter, AlertDialogBody, AlertDialogHeading, AlertDialogHeader, AlertDialogDialog, AlertDialogContainer, AlertDialogBackdrop, AlertDialogTrigger } from './alert-dialog.js';
export { alertDialogVariants } from '@heroui/styles';

/* -------------------------------------------------------------------------------------------------
 * Compound Component
 * -----------------------------------------------------------------------------------------------*/
const AlertDialog = Object.assign(AlertDialogRoot, {
  Root: AlertDialogRoot,
  Trigger: AlertDialogTrigger,
  Backdrop: AlertDialogBackdrop,
  Container: AlertDialogContainer,
  Dialog: AlertDialogDialog,
  Header: AlertDialogHeader,
  Heading: AlertDialogHeading,
  Body: AlertDialogBody,
  Footer: AlertDialogFooter,
  Icon: AlertDialogIcon,
  CloseTrigger: AlertDialogCloseTrigger
});

export { AlertDialog, AlertDialogBackdrop, AlertDialogBody, AlertDialogCloseTrigger, AlertDialogContainer, AlertDialogDialog, AlertDialogFooter, AlertDialogHeader, AlertDialogHeading, AlertDialogIcon, AlertDialogRoot, AlertDialogTrigger };
