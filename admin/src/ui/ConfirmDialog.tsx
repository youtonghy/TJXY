import { Alert, Button, Modal } from '@heroui/react';
import { TriangleAlert } from 'lucide-react';
import { useEffect, useRef, useState, type ReactNode } from 'react';

export interface ConfirmDialogProps {
  trigger: ReactNode;
  title: string;
  description: ReactNode;
  confirmLabel: string;
  isPending: boolean;
  onConfirm: () => void | Promise<void>;
}

export function ConfirmDialog({
  trigger,
  title,
  description,
  confirmLabel,
  isPending,
  onConfirm,
}: ConfirmDialogProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [hasSubmissionError, setHasSubmissionError] = useState(false);
  const triggerContainerRef = useRef<HTMLSpanElement>(null);
  const shouldRestoreFocusRef = useRef(false);
  const isLocked = isPending || isSubmitting;

  useEffect(() => {
    if (isOpen || !shouldRestoreFocusRef.current) return;
    shouldRestoreFocusRef.current = false;
    triggerContainerRef.current
      ?.querySelector<HTMLElement>('button, [href], [tabindex]:not([tabindex="-1"])')
      ?.focus();
  }, [isOpen]);

  const handleOpenChange = (nextOpen: boolean) => {
    if (isLocked) return;
    setIsOpen(nextOpen);
    if (nextOpen) setHasSubmissionError(false);
  };

  const handleConfirm = async () => {
    if (isLocked) return;
    setHasSubmissionError(false);
    setIsSubmitting(true);
    try {
      await onConfirm();
      shouldRestoreFocusRef.current = true;
      setIsOpen(false);
    } catch {
      setHasSubmissionError(true);
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <Modal isOpen={isOpen} onOpenChange={handleOpenChange}>
      <span className="contents" ref={triggerContainerRef}>{trigger}</span>
      <Modal.Backdrop
        isDismissable={!isLocked}
        isKeyboardDismissDisabled={isLocked}
      >
        <Modal.Container placement="center" size="sm">
          <Modal.Dialog>
            <Modal.CloseTrigger aria-label="Close" isDisabled={isLocked} />
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
                    <Alert.Title>The action could not be completed</Alert.Title>
                    <Alert.Description>Review the current state and try again.</Alert.Description>
                  </Alert.Content>
                </Alert>
              )}
            </Modal.Body>
            <Modal.Footer>
              <Button autoFocus isDisabled={isLocked} slot="close" variant="tertiary">
                Cancel
              </Button>
              <Button
                aria-busy={isLocked}
                isDisabled={isLocked}
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
