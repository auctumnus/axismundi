import { Description, Dialog, DialogBackdrop, DialogPanel, DialogTitle, CloseButton as CloseButtonOriginal } from '@headlessui/react'
import { useState } from 'react';

export interface ModalInnerProps {
    open: boolean;
    close: () => void;
    title: string;
    contents: (close: () => void) => React.ReactNode;
}

export function ModalInner({ open, close, title, contents } : ModalInnerProps) {
  return (
    <Dialog open={open} onClose={close} className="modal-container">
      <DialogBackdrop transition className="modal-backdrop" />

        {/* Full-screen container to center the panel */}
        <div className="modal-inner-container">
          {/* The actual dialog panel  */}
          <DialogPanel transition className="modal-panel">
            <DialogTitle className="modal-title">{title}</DialogTitle>
            {contents(close)}
          </DialogPanel>
        </div>
      </Dialog>
  )
}

export interface ModalProps {
    title: string;
    button: string;
    contents: (close: () => void) => React.ReactNode;
}

export function Modal({ title, button, contents }: ModalProps) {
    const [open, setOpen] = useState(false);

    const close = () => setOpen(false);
    const openModal = () => setOpen(true);

    return (
        <>
            <button type="button" className="normal" onClick={openModal}>{button}</button>
            <ModalInner open={open} close={close} title={title} contents={contents} />
        </>
    );
}

export const CloseButton = CloseButtonOriginal;