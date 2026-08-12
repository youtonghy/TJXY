import { Alert, Button, Modal } from '@heroui/react';
import { TriangleAlert } from 'lucide-react';
import { useEffect, useRef, useState, type ReactElement, type ReactNode } from 'react';

export interface ConfirmDialogProps {
  trigger: ReactElement;
  title: string;
  description: ReactNode;
  /** Caller-vetted copy for known failures; never pass a raw server message. */
  errorDescription?: ReactNode;
  confirmLabel: string;
  isPending: boolean;
  onConfirm: () => void | Promise<void>;
  cancelLabel?: string;
  closeLabel?: string;
  errorTitle?: string;
}

export function ConfirmDialog({
  trigger,
  title,
  description,
  errorDescription,
  confirmLabel,
  isPending,
  onConfirm,
  cancelLabel = 'Cancel',
  closeLabel = 'Close',
  errorTitle = 'The action could not be completed',
}: ConfirmDialogProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [hasSubmissionError, setHasSubmissionError] = useState(false);
  const triggerRef = useRef<HTMLElement | null>(null);
  const shouldRestoreFocusRef = useRef(false);
  const submissionRef = useRef(false);
  const isLocked = isPending || isSubmitting;

  useEffect(() => {
    if (isOpen || !shouldRestoreFocusRef.current) return;
    shouldRestoreFocusRef.current = false;
    triggerRef.current?.focus();
  }, [isOpen]);

  const handleOpenChange = (nextOpen: boolean) => {
    if (isLocked) return;
    if (nextOpen && document.activeElement instanceof HTMLElement) {
      triggerRef.current = document.activeElement;
    }
    setIsOpen(nextOpen);
    if (nextOpen) setHasSubmissionError(false);
  };

  const handleConfirm = async () => {
    if (isLocked || submissionRef.current) return;
    submissionRef.current = true;
    setHasSubmissionError(false);
    setIsSubmitting(true);
    try {
      await onConfirm();
      shouldRestoreFocusRef.current = true;
      setIsOpen(false);
    } catch {
      setHasSubmissionError(true);
    } finally {
      submissionRef.current = false;
      setIsSubmitting(false);
    }
  };

  return (
    <Modal isOpen={isOpen} onOpenChange={handleOpenChange}>
      {trigger}
      <Modal.Backdrop
        isDismissable={!isLocked}
        isKeyboardDismissDisabled={isLocked}
      >
        <Modal.Container placement="center" size="sm">
          <Modal.Dialog>
            <Modal.CloseTrigger aria-label={closeLabel} isDisabled={isLocked} />
            <Modal.Header>
              <Modal.Heading>{title}</Modal.Heading>
            </Modal.Header>
            <Modal.Body>
              <div className="text-sm leading-6 text-muted">{description}</div>
              {hasSubmissionError && (
                <Alert className="mt-3" role="alert" status="danger">
                  <Alert.Indicator>
                    <TriangleAlert aria-hidden="true" className="size-4" />
                  </Alert.Indicator>
                  <Alert.Content>
                    <Alert.Title>{errorTitle}</Alert.Title>
                    <Alert.Description>
                      {errorDescription ?? 'Review the current state and try again.'}
                    </Alert.Description>
                  </Alert.Content>
                </Alert>
              )}
            </Modal.Body>
            <Modal.Footer>
              <Button autoFocus isDisabled={isLocked} slot="close" variant="tertiary">
                {cancelLabel}
              </Button>
              <Button
                isPending={isLocked}
                onPress={() => { void handleConfirm(); }}
                variant="danger"
              >
                <span className="inline-flex min-h-5 items-center">{confirmLabel}</span>
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}
