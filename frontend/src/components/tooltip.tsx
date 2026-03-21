import {
    useFloating, useHover, useFocus, useDismiss, useRole,
    useInteractions, autoUpdate, offset, flip, FloatingPortal
} from "@floating-ui/react";
import { useState } from "react";

export const Tooltip = ({ content, children }: { content: string; children: React.ReactNode }) => {
    const [isOpen, setIsOpen] = useState(false);

    const { refs, floatingStyles, context } = useFloating({
        open: isOpen,
        onOpenChange: setIsOpen,
        placement: "bottom",
        middleware: [offset(4), flip()],
        whileElementsMounted: autoUpdate,
    });

    const hover = useHover(context, { move: false });
    const focus = useFocus(context);
    const dismiss = useDismiss(context);
    const role = useRole(context, { role: "tooltip" });
    const { getReferenceProps, getFloatingProps } = useInteractions([hover, focus, dismiss, role]);

    return (
        <>
            <span style={{ "display": "inline-flex" }} ref={refs.setReference} {...getReferenceProps()}>
                {children}
            </span>
            {
                isOpen ?
                <FloatingPortal>
                    <span
                        className={`tooltip${isOpen ? " visible" : ""}`}
                        ref={refs.setFloating}
                        style={floatingStyles}
                        {...getFloatingProps()}
                    >
                        {content}
                    </span>
                </FloatingPortal>
                : <></>
            }
        </>
    );
};
