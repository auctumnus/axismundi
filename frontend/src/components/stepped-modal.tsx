import {
  Dialog,
  DialogBackdrop,
  DialogPanel,
  DialogTitle,
} from "@headlessui/react";
import { useEffect, useState } from "react";

export interface SteppedModalStep {
  title: string;
  content: (close: () => void) => React.ReactNode;
}

export interface SteppedModalProps {
  open: boolean;
  close: () => void;
  step: number;
  steps: SteppedModalStep[];
}

export function SteppedModal({ open, close, step, steps }: SteppedModalProps) {
  const [displayedStep, setDisplayedStep] = useState(step);
  const [fading, setFading] = useState(false);

  useEffect(() => {
    if (step !== displayedStep) {
      setFading(true);
      const timer = setTimeout(() => {
        setDisplayedStep(step);
        setFading(false);
      }, 150);
      return () => clearTimeout(timer);
    }
  }, [step]);

  // reset displayed step when modal opens
  useEffect(() => {
    if (open) {
      setDisplayedStep(step);
      setFading(false);
    }
  }, [open]);

  const current = steps[displayedStep];
  if (!current) return null;

  return (
    <Dialog open={open} onClose={close} className="modal-container">
      <DialogBackdrop transition className="modal-backdrop" />
      <div className="modal-inner-container">
        <DialogPanel transition className="modal-panel">
          <div className={`stepped-content${fading ? " faded" : ""}`}>
            <DialogTitle className="modal-title">{current.title}</DialogTitle>
            {current.content(close)}
          </div>
        </DialogPanel>
      </div>
    </Dialog>
  );
}
