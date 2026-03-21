import { useEditor, type EditorState, PRESETS } from "./state"
import { Tooltip } from "../components/tooltip"
import { getByPath, normalizeHeadingPath, type CellPath, type HeadingPath, type TablePath } from "./path";
import { numLeaves, type Cell, type Column, type Row, type TableElement, type TOP_LEFT_CELL } from "./table";
import { path } from "slate";
import { ModalInner } from "../components/modal";
import { SteppedModal, type SteppedModalStep } from "../components/stepped-modal";
import { useEffect, useState } from "react";

interface ControlState {
    selectTarget: TableElement | typeof TOP_LEFT_CELL | null;
    isRowSelected: boolean;
    isColumnSelected: boolean;
    isCellSelected: boolean;
}

interface ControlButtonProps {
    onClick: () => void;
    title: string;
    children: React.ReactNode;
    enabled: boolean;
}

const ControlButton = ({ onClick, title, children, enabled }: ControlButtonProps) => {
    return (
        <Tooltip content={title}>
            <button type="button" className={`control-button ${enabled ? "" : "disabled"}`} onClick={onClick} disabled={!enabled}>
                {children}
            </button>
        </Tooltip>
    )
}

interface EditHeadingModalProps {
    kind: "row" | "column";
    currentHeading: string;
    onSave: (newHeading: string) => void;
    onClose: () => void;
    open: boolean;
}

const EditHeadingModal = ({ kind, currentHeading, onSave, onClose, open }: EditHeadingModalProps) => {
    const [newHeading, setNewHeading] = useState(currentHeading);

    const handleSave = () => {
        onSave(newHeading);
        onClose();
    }

    return (
        <ModalInner open={open} close={onClose} title={`Edit ${kind} heading`} contents={(close) => (<>
            <section>
                <label htmlFor={`${kind}-heading`}>Name</label>
                <input name={`${kind}-heading`} className="normal" type="text" value={newHeading} onChange={(e) => setNewHeading(e.target.value)} autoFocus onKeyDown={(e) => e.key === "Enter" && newHeading.trim() && handleSave()} />
            </section>
            <div className="button-row">
                <button type="button" className="normal secondary" onClick={close}>Cancel</button>
                <button type="button" className="normal" onClick={handleSave} disabled={!newHeading.trim()}>Save</button>
            </div>
        </>)}/>
    )
}

/*

todo:
- draw the rest of the owl in regards to actual functionality
- check that the enable logic is right for each button
- modals for editing headings, phonemes, annotations
- better disabled styling?
- tooltips for disabled buttons explaining why they're disabled?

*/

const RowControls = ({ isRowSelected, selectTarget }: ControlState) => {
    const [state, dispatch] = useEditor();

    const groupSelected = isRowSelected && selectTarget && "rows" in (selectTarget as Row);
    const onlyOneRow = (numLeaves(state.body.rows) === 1) && !groupSelected;

    const p = (cb: (path: HeadingPath) => any) => {
        if (state.select && state.select.type === "RowHeading") {
            const path = normalizeHeadingPath(state.body.rows, state.select.path)!
            cb(path)
        }
    }

    const [editModalOpen, setEditModalOpen] = useState(false);
    const selectedRow = isRowSelected && state.select?.type === "RowHeading"
        ? getByPath(state.body, state.select) as Row | null
        : null;

    const editRowHeading = () => {
        if (isRowSelected) setEditModalOpen(true);
    }

    useEffect(() => {
        if (state.pendingModal === "EditRowHeading") {
            dispatch({ type: "ClearPendingModal" });
            editRowHeading();
        }
    }, [state.pendingModal]);

    const addRowAbove = () => p((path: HeadingPath) => dispatch({
        "type": "AddHeading", path, kind: "row", position: "before"
    }))
    const addRowBelow = () => p((path: HeadingPath) => dispatch({
        "type": "AddHeading", path, kind: "row", position: "after"
    }))
    const splitRow = () => p((path: HeadingPath) => dispatch({
        "type": "SplitHeading", path, kind: "row"
    }))
    const deleteRow = () => p((path: HeadingPath) => dispatch({
        "type": "DeleteHeading", path, kind: "row"
    }))

    return (
        <div className="controls row-controls">
            <span className="controls-header">Row</span>
            {selectedRow && (
                <EditHeadingModal
                    key={selectedRow.heading}
                    kind="row"
                    currentHeading={selectedRow.heading}
                    open={editModalOpen}
                    onClose={() => setEditModalOpen(false)}
                    onSave={(newHeading) => p((path) => dispatch({
                        type: "EditHeading", kind: "row", path, newHeading
                    }))}
                />
            )}
            <ControlButton onClick={editRowHeading} title="Edit row heading" enabled={isRowSelected}>
                <svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24">{/* Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE */}<path fill="currentColor" d="M5 19h1.425L16.2 9.225L14.775 7.8L5 17.575zm-1 2q-.425 0-.712-.288T3 20v-2.425q0-.4.15-.763t.425-.637L16.2 3.575q.3-.275.663-.425t.762-.15t.775.15t.65.45L20.425 5q.3.275.437.65T21 6.4q0 .4-.138.763t-.437.662l-12.6 12.6q-.275.275-.638.425t-.762.15zM19 6.4L17.6 5zm-3.525 2.125l-.7-.725L16.2 9.225z" /></svg>
            </ControlButton>
            <ControlButton onClick={addRowAbove} title="Add row above" enabled={isRowSelected}>
                <svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24">{/* Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE */}<path fill="currentColor" d="M5 14v6h14v-6zm0 8q-.825 0-1.412-.587T3 20V6q0-.825.588-1.412T5 4h1q.425 0 .713.288T7 5t-.288.713T6 6H5v6h14V6h-1q-.425 0-.712-.288T17 5t.288-.712T18 4h1q.825 0 1.413.588T21 6v14q0 .825-.587 1.413T19 22zm6-16h-1q-.425 0-.712-.288T9 5t.288-.712T10 4h1V3q0-.425.288-.712T12 2t.713.288T13 3v1h1q.425 0 .713.288T15 5t-.288.713T14 6h-1v1q0 .425-.288.713T12 8t-.712-.288T11 7zm1 8v-2zm0 0" /></svg>
            </ControlButton>
            <ControlButton onClick={addRowBelow} title="Add row below" enabled={isRowSelected}>
                <svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24">{/* Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE */}<path fill="currentColor" d="M19 10V4H5v6zM5 20q-.825 0-1.412-.587T3 18V4q0-.825.588-1.412T5 2h14q.825 0 1.413.588T21 4v14q0 .825-.587 1.413T19 20h-1q-.425 0-.712-.288T17 19t.288-.712T18 18h1v-6H5v6h1q.425 0 .713.288T7 19t-.288.713T6 20zm7 2q-.425 0-.712-.288T11 21v-1h-1q-.425 0-.712-.288T9 19t.288-.712T10 18h1v-1q0-.425.288-.712T12 16t.713.288T13 17v1h1q.425 0 .713.288T15 19t-.288.713T14 20h-1v1q0 .425-.288.713T12 22m0-12v2zm0 0" /></svg>
            </ControlButton>
            <ControlButton onClick={splitRow} title="Split row" enabled={isRowSelected}>
                <svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24">{/* Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE */}<path fill="currentColor" d="M5 21q-.825 0-1.412-.587T3 19v-4q0-.825.588-1.412T5 13h14q.825 0 1.413.588T21 15v4q0 .825-.587 1.413T19 21zm0-10q-.825 0-1.412-.587T3 9V5q0-.825.588-1.412T5 3h14q.825 0 1.413.588T21 5v4q0 .825-.587 1.413T19 11zm14-6H5v4h14zM5 9V5z" /></svg>
            </ControlButton>
            <ControlButton onClick={deleteRow} title="Delete row" enabled={isRowSelected && !onlyOneRow}>
                <svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24">{/* Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE */}<path fill="currentColor" d="M7 21q-.825 0-1.412-.587T5 19V6q-.425 0-.712-.288T4 5t.288-.712T5 4h4q0-.425.288-.712T10 3h4q.425 0 .713.288T15 4h4q.425 0 .713.288T20 5t-.288.713T19 6v13q0 .825-.587 1.413T17 21zM17 6H7v13h10zm-7 11q.425 0 .713-.288T11 16V9q0-.425-.288-.712T10 8t-.712.288T9 9v7q0 .425.288.713T10 17m4 0q.425 0 .713-.288T15 16V9q0-.425-.288-.712T14 8t-.712.288T13 9v7q0 .425.288.713T14 17M7 6v13z" /></svg>
            </ControlButton>
        </div>
    )
}

const ColumnControls = ({ isColumnSelected, selectTarget }: ControlState) => {
    const [state, dispatch] = useEditor();

    const groupSelected = isColumnSelected && selectTarget && "columns" in (selectTarget as Column);

    const onlyOneColumn = (numLeaves(state.body.columns) === 1) && !groupSelected;

    const p = (cb: (path: HeadingPath) => any) => {
        if (state.select && state.select.type === "ColumnHeading") {
            const path = normalizeHeadingPath(state.body.columns, state.select.path)!
            cb(path)
        }
    }

    const [editModalOpen, setEditModalOpen] = useState(false);
    const selectedColumn = isColumnSelected && state.select?.type === "ColumnHeading"
        ? getByPath(state.body, state.select) as Column | null
        : null;

    const editColumnHeading = () => {
        if (isColumnSelected) setEditModalOpen(true);
    }

    useEffect(() => {
        if (state.pendingModal === "EditColumnHeading") {
            dispatch({ type: "ClearPendingModal" });
            editColumnHeading();
        }
    }, [state.pendingModal]);

    const addColumnLeft = () => p((path: HeadingPath) => dispatch({
        "type": "AddHeading", path, kind: "column", position: "before"
    }))
    const addColumnRight = () => p((path: HeadingPath) => dispatch({
        "type": "AddHeading", path, kind: "column", position: "after"
    }))
    const splitColumn = () => p((path: HeadingPath) => dispatch({
        "type": "SplitHeading", path, kind: "column"
    }))
    const deleteColumn = () => p((path: HeadingPath) => dispatch({
        "type": "DeleteHeading", path, kind: "column"
    }))

    return (
        <div className="controls row-controls">
            <span className="controls-header">Column</span>
            {selectedColumn && (
                <EditHeadingModal
                    key={selectedColumn.heading}
                    kind="column"
                    currentHeading={selectedColumn.heading}
                    open={editModalOpen}
                    onClose={() => setEditModalOpen(false)}
                    onSave={(newHeading) => p((path) => dispatch({
                        type: "EditHeading", kind: "column", path, newHeading
                    }))}
                />
            )}
            <ControlButton onClick={editColumnHeading} title="Edit column heading" enabled={isColumnSelected}>
                <svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24">{/* Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE */}<path fill="currentColor" d="M5 19h1.425L16.2 9.225L14.775 7.8L5 17.575zm-1 2q-.425 0-.712-.288T3 20v-2.425q0-.4.15-.763t.425-.637L16.2 3.575q.3-.275.663-.425t.762-.15t.775.15t.65.45L20.425 5q.3.275.437.65T21 6.4q0 .4-.138.763t-.437.662l-12.6 12.6q-.275.275-.638.425t-.762.15zM19 6.4L17.6 5zm-3.525 2.125l-.7-.725L16.2 9.225z" /></svg>
            </ControlButton>
            <ControlButton onClick={addColumnLeft} title="Add column left" enabled={isColumnSelected}>
                <svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24">{/* Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE */}<path fill="currentColor" d="M14 19h6V5h-6zm-8 2q-.825 0-1.412-.587T4 19v-1q0-.425.288-.712T5 17t.713.288T6 18v1h6V5H6v1q0 .425-.288.713T5 7t-.712-.288T4 6V5q0-.825.588-1.412T6 3h14q.825 0 1.413.588T22 5v14q0 .825-.587 1.413T20 21zm-1-6q-.425 0-.712-.288T4 14v-1H3q-.425 0-.712-.288T2 12t.288-.712T3 11h1v-1q0-.425.288-.712T5 9t.713.288T6 10v1h1q.425 0 .713.288T8 12t-.288.713T7 13H6v1q0 .425-.288.713T5 15m9-3h-2zm0 0" /></svg>    
            </ControlButton>
            <ControlButton onClick={addColumnRight} title="Add column right" enabled={isColumnSelected}>
                <svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24">{/* Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE */}<path fill="currentColor" d="M10 5H4v14h6zM4 21q-.825 0-1.412-.587T2 19V5q0-.825.588-1.412T4 3h14q.825 0 1.413.588T20 5v1q0 .425-.288.713T19 7t-.712-.288T18 6V5h-6v14h6v-1q0-.425.288-.712T19 17t.713.288T20 18v1q0 .825-.587 1.413T18 21zm15-6q-.425 0-.712-.288T18 14v-1h-1q-.425 0-.712-.288T16 12t.288-.712T17 11h1v-1q0-.425.288-.712T19 9t.713.288T20 10v1h1q.425 0 .713.288T22 12t-.288.713T21 13h-1v1q0 .425-.288.713T19 15m-9-3h2zm0 0" /></svg>
            </ControlButton>
            <ControlButton onClick={splitColumn} title="Split column" enabled={isColumnSelected}>
                <svg xmlns="http://www.w3.org/2000/svg" style={{ "transform": "rotate(90deg)"}} width="1em" height="1em" viewBox="0 0 24 24">{/* Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE */}<path fill="currentColor" d="M5 21q-.825 0-1.412-.587T3 19v-4q0-.825.588-1.412T5 13h14q.825 0 1.413.588T21 15v4q0 .825-.587 1.413T19 21zm0-10q-.825 0-1.412-.587T3 9V5q0-.825.588-1.412T5 3h14q.825 0 1.413.588T21 5v4q0 .825-.587 1.413T19 11zm14-6H5v4h14zM5 9V5z" /></svg>
            </ControlButton>
            <ControlButton onClick={deleteColumn} title="Delete column" enabled={isColumnSelected && !onlyOneColumn}>
                <svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24">{/* Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE */}<path fill="currentColor" d="M7 21q-.825 0-1.412-.587T5 19V6q-.425 0-.712-.288T4 5t.288-.712T5 4h4q0-.425.288-.712T10 3h4q.425 0 .713.288T15 4h4q.425 0 .713.288T20 5t-.288.713T19 6v13q0 .825-.587 1.413T17 21zM17 6H7v13h10zm-7 11q.425 0 .713-.288T11 16V9q0-.425-.288-.712T10 8t-.712.288T9 9v7q0 .425.288.713T10 17m4 0q.425 0 .713-.288T15 16V9q0-.425-.288-.712T14 8t-.712.288T13 9v7q0 .425.288.713T14 17M7 6v13z" /></svg>
            </ControlButton>
        </div>
    )
}


interface PhonemeEditModalProps {
    kind: "edit" | "add";
    phonemeIndex: number;
    onSave: (newPhoneme: string) => void;
    onClose: () => void;
    open: boolean;
}

const PhonemeEditModal = ({ kind, phonemeIndex, onSave, onClose, open }: PhonemeEditModalProps) => {
    const [state, dispatch] = useEditor();

    const cell = state.select && state.select.type === "Cell"
        ? getByPath(state.body, state.select) as Cell | null
        : null;

    const [newPhoneme, setNewPhoneme] = useState(
        kind === "edit" && cell ? cell.phonemes[phonemeIndex]!.text : ""
    );

    if (!cell) return <></>;

    const title = kind === "edit" ? `Edit phoneme` : `Add phoneme`;


    return (
        <ModalInner open={open} close={onClose} title={title} contents={(close) => (<>
            <section>
                <label htmlFor="phoneme-text">Phoneme</label>
                <input name="phoneme-text" className="normal" type="text" value={newPhoneme} onChange={(e) => setNewPhoneme(e.target.value)} autoFocus onKeyDown={(e) => e.key === "Enter" && newPhoneme.trim() && onSave(newPhoneme)} />
            </section>
            <div className="button-row">
                <button type="button" className="normal secondary" onClick={close}>Cancel</button>
                <button type="button" className="normal" onClick={() => onSave(newPhoneme)} disabled={!newPhoneme.trim()}>Save</button>
            </div>
        </>)}/>
    )
}

const PhonemeControls = ({ isCellSelected, selectTarget }: ControlState) => {
    const [state, dispatch] = useEditor();

    const cell = isCellSelected ? selectTarget as Cell : null;
    const selectedHasPhoneme = cell !== null && cell.phonemes.length > 0;
    const hasMultiplePhonemes = cell !== null && cell.phonemes.length > 1;

    // stepped modal state
    const [modalOpen, setModalOpen] = useState(false);
    const [modalStep, setModalStep] = useState(0);
    const [modalPurpose, setModalPurpose] = useState<'edit' | 'delete'>('edit');
    const [selectedPhonemeIndex, setSelectedPhonemeIndex] = useState(0);
    const [addModalOpen, setAddModalOpen] = useState(false);

    // for the edit step, we need local state for the input
    const [editText, setEditText] = useState('');

    const closeModal = () => {
        setModalOpen(false);
        setModalStep(0);
    }

    const addPhoneme = () => {
        if (isCellSelected) setAddModalOpen(true);
    }

    const editPhoneme = () => {
        if (!selectedHasPhoneme || !state.select) return;
        setModalPurpose('edit');
        if (hasMultiplePhonemes) {
            setModalStep(0);
            setModalOpen(true);
        } else {
            // skip select, go straight to edit
            setSelectedPhonemeIndex(0);
            setEditText(cell!.phonemes[0]!.text);
            setModalStep(1);
            setModalOpen(true);
        }
    }

    const deletePhoneme = () => {
        if (!selectedHasPhoneme || !state.select) return;
        if (hasMultiplePhonemes) {
            setModalPurpose('delete');
            setModalStep(0);
            setModalOpen(true);
        } else {
            dispatch({ type: "RemovePhoneme", path: state.select, index: 0 });
        }
    }

    const handlePhonemeSelected = (index: number) => {
        if (modalPurpose === 'edit') {
            setSelectedPhonemeIndex(index);
            setEditText(cell!.phonemes[index]!.text);
            setModalStep(1);
        } else {
            if (state.select) {
                dispatch({ type: "RemovePhoneme", path: state.select, index });
            }
            closeModal();
        }
    }

    const handleEditSave = () => {
        if (!state.select) return;
        dispatch({ type: "EditPhoneme", path: state.select, index: selectedPhonemeIndex, newText: editText });
        closeModal();
    }

    const handleAddSave = (newPhoneme: string) => {
        if (!state.select) return;
        dispatch({ type: "AddPhoneme", phoneme: newPhoneme, path: state.select });
        setAddModalOpen(false);
    }

    useEffect(() => {
        if (state.pendingModal === "AddPhoneme") {
            dispatch({ type: "ClearPendingModal" });
            addPhoneme();
        } else if (state.pendingModal === "EditPhoneme") {
            dispatch({ type: "ClearPendingModal" });
            editPhoneme();
        } else if (state.pendingModal === "DeletePhoneme") {
            dispatch({ type: "ClearPendingModal" });
            deletePhoneme();
        }
    }, [state.pendingModal]);

    const selectStep = {
        title: modalPurpose === 'edit' ? 'select phoneme' : 'delete phoneme',
        content: (close: () => void) => (
            <>
                <section className="phoneme-options">
                    {cell?.phonemes.map((phoneme, index) => (
                        <button key={index} type="button" className="normal phoneme-option" onClick={() => handlePhonemeSelected(index)}>
                            {phoneme.text}
                        </button>
                    ))}
                </section>
                <div className="button-row">
                    <button type="button" className="normal secondary" onClick={close}>Cancel</button>
                </div>
            </>
        ),
    }

    const editStep = {
        title: 'edit phoneme',
        content: (close: () => void) => (
            <>
                <section>
                    <label htmlFor="phoneme-text">Phoneme</label>
                    <input name="phoneme-text" className="normal" type="text" value={editText} onChange={(e) => setEditText(e.target.value)} autoFocus onKeyDown={(e) => e.key === "Enter" && editText.trim() && handleEditSave()} />
                </section>
                <div className="button-row">
                    <button type="button" className="normal secondary" onClick={close}>Cancel</button>
                    <button type="button" className="normal" onClick={handleEditSave} disabled={!editText.trim()}>Save</button>
                </div>
            </>
        ),
    }

    return (
        <div className="controls phoneme-controls">
            <span className="controls-header">Phonemes</span>
            <SteppedModal
                open={modalOpen}
                close={closeModal}
                step={modalStep}
                steps={[selectStep, editStep]}
            />
            <PhonemeEditModal
                key={`add-${addModalOpen}`}
                kind="add"
                phonemeIndex={0}
                open={addModalOpen}
                onClose={() => setAddModalOpen(false)}
                onSave={handleAddSave}
            />
            <ControlButton onClick={addPhoneme} title="Add phoneme" enabled={isCellSelected}>
                <svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24">{/* Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE */}<path fill="currentColor" d="M11 13H6q-.425 0-.712-.288T5 12t.288-.712T6 11h5V6q0-.425.288-.712T12 5t.713.288T13 6v5h5q.425 0 .713.288T19 12t-.288.713T18 13h-5v5q0 .425-.288.713T12 19t-.712-.288T11 18z" /></svg>
            </ControlButton>
            <ControlButton onClick={editPhoneme} title="Edit phoneme" enabled={selectedHasPhoneme}>
                <svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24">{/* Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE */}<path fill="currentColor" d="M5 19h1.425L16.2 9.225L14.775 7.8L5 17.575zm-1 2q-.425 0-.712-.288T3 20v-2.425q0-.4.15-.763t.425-.637L16.2 3.575q.3-.275.663-.425t.762-.15t.775.15t.65.45L20.425 5q.3.275.437.65T21 6.4q0 .4-.138.763t-.437.662l-12.6 12.6q-.275.275-.638.425t-.762.15zM19 6.4L17.6 5zm-3.525 2.125l-.7-.725L16.2 9.225z" /></svg>
            </ControlButton>
            <ControlButton onClick={deletePhoneme} title="Delete phoneme" enabled={selectedHasPhoneme}>
                <svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24">{/* Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE */}<path fill="currentColor" d="M7 21q-.825 0-1.412-.587T5 19V6q-.425 0-.712-.288T4 5t.288-.712T5 4h4q0-.425.288-.712T10 3h4q.425 0 .713.288T15 4h4q.425 0 .713.288T20 5t-.288.713T19 6v13q0 .825-.587 1.413T17 21zM17 6H7v13h10zm-7 11q.425 0 .713-.288T11 16V9q0-.425-.288-.712T10 8t-.712.288T9 9v7q0 .425.288.713T10 17m4 0q.425 0 .713-.288T15 16V9q0-.425-.288-.712T14 8t-.712.288T13 9v7q0 .425.288.713T14 17M7 6v13z" /></svg>
            </ControlButton>
        </div>
    )
}

const AnnotationControls = ({ isCellSelected, selectTarget }: ControlState) => {
    const [state, dispatch] = useEditor();

    const cell = isCellSelected ? selectTarget as Cell : null;
    const hasPhonemes = cell !== null && cell.phonemes.length > 0;
    const hasMultiplePhonemes = cell !== null && cell.phonemes.length > 1;
    const selectedHasAnnotation = isCellSelected && (selectTarget as Cell).phonemes.some(p => p.annotations.length > 0);
    const hasTableAnnotations = state.body.annotations.length > 0;

    // modal state
    const [modalOpen, setModalOpen] = useState(false);
    const [modalStep, setModalStep] = useState(0);
    const [modalPurpose, setModalPurpose] = useState<'add' | 'link' | 'edit' | 'delete'>('add');
    const [selectedPhonemeIndex, setSelectedPhonemeIndex] = useState(0);
    const [selectedAnnotationIndex, setSelectedAnnotationIndex] = useState(0);
    const [annotationText, setAnnotationText] = useState('');

    const closeModal = () => {
        setModalOpen(false);
        setModalStep(0);
    }

    // --- step handlers ---

    const handlePhonemeSelected = (index: number) => {
        setSelectedPhonemeIndex(index);
        const phoneme = cell!.phonemes[index]!;

        if (modalPurpose === 'add') {
            setAnnotationText('');
            setModalStep(1);
        } else if (modalPurpose === 'link') {
            setModalStep(1);
        } else if (modalPurpose === 'edit') {
            if (phoneme.annotations.length === 0) {
                closeModal();
            } else if (phoneme.annotations.length === 1) {
                const annIdx = phoneme.annotations[0]!;
                setSelectedAnnotationIndex(annIdx);
                setAnnotationText(state.body.annotations[annIdx]!);
                setModalStep(2);
            } else {
                setModalStep(1);
            }
        } else if (modalPurpose === 'delete') {
            if (phoneme.annotations.length === 0) {
                closeModal();
            } else if (phoneme.annotations.length === 1) {
                dispatch({ type: "RemoveAnnotation", path: state.select!, phonemeIndex: index, annotationIndex: phoneme.annotations[0]! });
                closeModal();
            } else {
                setModalStep(1);
            }
        }
    }

    const handleAnnotationSelected = (annotationIndex: number) => {
        if (modalPurpose === 'delete') {
            dispatch({ type: "RemoveAnnotation", path: state.select!, phonemeIndex: selectedPhonemeIndex, annotationIndex });
            closeModal();
        } else if (modalPurpose === 'edit') {
            setSelectedAnnotationIndex(annotationIndex);
            setAnnotationText(state.body.annotations[annotationIndex]!);
            setModalStep(2);
        }
    }

    const handleLinkSelected = (annotationIndex: number) => {
        if (!state.select) return;
        dispatch({ type: "LinkAnnotation", path: state.select, phonemeIndex: selectedPhonemeIndex, annotationIndex });
        closeModal();
    }

    const handleAddSave = () => {
        if (!state.select || !annotationText.trim()) return;
        dispatch({ type: "AddNewAnnotation", annotation: annotationText.trim(), path: state.select, phonemeIndex: selectedPhonemeIndex });
        closeModal();
    }

    const handleEditSave = () => {
        dispatch({ type: "EditAnnotation", annotationIndex: selectedAnnotationIndex, newText: annotationText });
        closeModal();
    }

    // --- entry points ---

    const addAnnotation = () => {
        if (!hasPhonemes || !state.select) return;
        setModalPurpose('add');
        setAnnotationText('');
        if (hasMultiplePhonemes) {
            setModalStep(0);
            setModalOpen(true);
        } else {
            setSelectedPhonemeIndex(0);
            setModalStep(1);
            setModalOpen(true);
        }
    }

    const linkAnnotation = () => {
        if (!hasPhonemes || !state.select) return;
        setModalPurpose('link');
        if (hasMultiplePhonemes) {
            setModalStep(0);
            setModalOpen(true);
        } else {
            setSelectedPhonemeIndex(0);
            setModalStep(1);
            setModalOpen(true);
        }
    }

    const editAnnotation = () => {
        if (!hasPhonemes || !state.select) return;
        setModalPurpose('edit');
        if (hasMultiplePhonemes) {
            setModalStep(0);
            setModalOpen(true);
        } else {
            const phoneme = cell!.phonemes[0]!;
            setSelectedPhonemeIndex(0);
            if (phoneme.annotations.length === 0) return;
            if (phoneme.annotations.length === 1) {
                const annIdx = phoneme.annotations[0]!;
                setSelectedAnnotationIndex(annIdx);
                setAnnotationText(state.body.annotations[annIdx]!);
                setModalStep(2);
                setModalOpen(true);
            } else {
                setModalStep(1);
                setModalOpen(true);
            }
        }
    }

    const deleteAnnotation = () => {
        if (!hasPhonemes || !state.select) return;
        setModalPurpose('delete');
        if (hasMultiplePhonemes) {
            setModalStep(0);
            setModalOpen(true);
        } else {
            const phoneme = cell!.phonemes[0]!;
            setSelectedPhonemeIndex(0);
            if (phoneme.annotations.length === 0) return;
            if (phoneme.annotations.length === 1) {
                dispatch({ type: "RemoveAnnotation", path: state.select, phonemeIndex: 0, annotationIndex: phoneme.annotations[0]! });
            } else {
                setModalStep(1);
                setModalOpen(true);
            }
        }
    }

    useEffect(() => {
        if (state.pendingModal === "AddAnnotation") {
            dispatch({ type: "ClearPendingModal" });
            addAnnotation();
        } else if (state.pendingModal === "LinkAnnotation") {
            dispatch({ type: "ClearPendingModal" });
            linkAnnotation();
        } else if (state.pendingModal === "EditAnnotation") {
            dispatch({ type: "ClearPendingModal" });
            editAnnotation();
        } else if (state.pendingModal === "DeleteAnnotation") {
            dispatch({ type: "ClearPendingModal" });
            deleteAnnotation();
        }
    }, [state.pendingModal]);

    // --- modal steps ---

    const selectPhonemeStep: SteppedModalStep = {
        title: 'select phoneme',
        content: (close) => (
            <>
                <section className="phoneme-options">
                    {cell?.phonemes.map((phoneme, index) => (
                        <button key={index} type="button" className="normal phoneme-option" onClick={() => handlePhonemeSelected(index)}>
                            {phoneme.text}
                        </button>
                    ))}
                </section>
                <div className="button-row">
                    <button type="button" className="normal secondary" onClick={close}>Cancel</button>
                </div>
            </>
        ),
    }

    const middleStep: SteppedModalStep = (() => {
        if (modalPurpose === 'add') {
            return {
                title: 'add annotation',
                content: (close: () => void) => (
                    <>
                        <section>
                            <label htmlFor="annotation-text">Annotation</label>
                            <input name="annotation-text" className="normal" type="text" value={annotationText} onChange={(e) => setAnnotationText(e.target.value)} autoFocus onKeyDown={(e) => e.key === "Enter" && annotationText.trim() && handleAddSave()} />
                        </section>
                        <div className="button-row">
                            <button type="button" className="normal secondary" onClick={close}>Cancel</button>
                            <button type="button" className="normal" onClick={handleAddSave} disabled={!annotationText.trim()}>Save</button>
                        </div>
                    </>
                ),
            };
        } else if (modalPurpose === 'link') {
            const phoneme = cell?.phonemes[selectedPhonemeIndex];
            const existingAnnotations = new Set(phoneme?.annotations ?? []);
            const availableAnnotations = state.body.annotations
                .map((text, index) => ({ text, index }))
                .filter(({ index }) => !existingAnnotations.has(index));

            return {
                title: 'link annotation',
                content: (close: () => void) => (
                    <>
                        <section className="phoneme-options">
                            {availableAnnotations.map(({ text, index }) => (
                                <button key={index} type="button" className="normal phoneme-option annotation-option" onClick={() => handleLinkSelected(index)}>
                                    {index + 1}. {text}
                                </button>
                            ))}
                            {availableAnnotations.length === 0 && <p>no annotations available to link</p>}
                        </section>
                        <div className="button-row">
                            <button type="button" className="normal secondary" onClick={close}>Cancel</button>
                        </div>
                    </>
                ),
            };
        } else {
            // edit or delete: select annotation from the phoneme
            const phoneme = cell?.phonemes[selectedPhonemeIndex];
            const annotations = phoneme?.annotations.map(i => ({ text: state.body.annotations[i]!, index: i })) ?? [];

            return {
                title: modalPurpose === 'edit' ? 'select annotation' : 'delete annotation',
                content: (close: () => void) => (
                    <>
                        <section className="phoneme-options">
                            {annotations.map(({ text, index }) => (
                                <button key={index} type="button" className="normal phoneme-option annotation-option" onClick={() => handleAnnotationSelected(index)}>
                                    {index + 1}. {text}
                                </button>
                            ))}
                        </section>
                        <div className="button-row">
                            <button type="button" className="normal secondary" onClick={close}>Cancel</button>
                        </div>
                    </>
                ),
            };
        }
    })();

    const editAnnotationStep: SteppedModalStep = {
        title: 'edit annotation',
        content: (close) => (
            <>
                <section>
                    <label htmlFor="annotation-text">Annotation</label>
                    <input name="annotation-text" className="normal" type="text" value={annotationText} onChange={(e) => setAnnotationText(e.target.value)} autoFocus onKeyDown={(e) => e.key === "Enter" && annotationText.trim() && handleEditSave()} />
                </section>
                <div className="button-row">
                    <button type="button" className="normal secondary" onClick={close}>Cancel</button>
                    <button type="button" className="normal" onClick={handleEditSave} disabled={!annotationText.trim()}>Save</button>
                </div>
            </>
        ),
    }

    return (
        <div className="controls annotation-controls">
            <span className="controls-header">Annotations</span>
            <SteppedModal
                open={modalOpen}
                close={closeModal}
                step={modalStep}
                steps={[selectPhonemeStep, middleStep, editAnnotationStep]}
            />
            <ControlButton onClick={addAnnotation} title="Add annotation" enabled={hasPhonemes}>
                <svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24">{/* Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE */}<path fill="currentColor" d="M11 13H6q-.425 0-.712-.288T5 12t.288-.712T6 11h5V6q0-.425.288-.712T12 5t.713.288T13 6v5h5q.425 0 .713.288T19 12t-.288.713T18 13h-5v5q0 .425-.288.713T12 19t-.712-.288T11 18z" /></svg>
            </ControlButton>
            <ControlButton onClick={linkAnnotation} title="Link annotation" enabled={hasPhonemes && hasTableAnnotations}>
                <svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24">{/* Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE */}<path fill="currentColor" d="M7 17q-2.075 0-3.537-1.463T2 12t1.463-3.537T7 7h3q.425 0 .713.288T11 8t-.288.713T10 9H7q-1.25 0-2.125.875T4 12t.875 2.125T7 15h3q.425 0 .713.288T11 16t-.288.713T10 17zm2-4q-.425 0-.712-.288T8 12t.288-.712T9 11h6q.425 0 .713.288T16 12t-.288.713T15 13zm5 4q-.425 0-.712-.288T13 16t.288-.712T14 15h3q1.25 0 2.125-.875T20 12t-.875-2.125T17 9h-3q-.425 0-.712-.288T13 8t.288-.712T14 7h3q2.075 0 3.538 1.463T22 12t-1.463 3.538T17 17z" /></svg>
            </ControlButton>
            <ControlButton onClick={editAnnotation} title="Edit annotation" enabled={selectedHasAnnotation}>
                <svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24">{/* Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE */}<path fill="currentColor" d="M5 19h1.425L16.2 9.225L14.775 7.8L5 17.575zm-1 2q-.425 0-.712-.288T3 20v-2.425q0-.4.15-.763t.425-.637L16.2 3.575q.3-.275.663-.425t.762-.15t.775.15t.65.45L20.425 5q.3.275.437.65T21 6.4q0 .4-.138.763t-.437.662l-12.6 12.6q-.275.275-.638.425t-.762.15zM19 6.4L17.6 5zm-3.525 2.125l-.7-.725L16.2 9.225z" /></svg>
            </ControlButton>
            <ControlButton onClick={deleteAnnotation} title="Delete annotation" enabled={selectedHasAnnotation}>
                <svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24">{/* Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE */}<path fill="currentColor" d="M7 21q-.825 0-1.412-.587T5 19V6q-.425 0-.712-.288T4 5t.288-.712T5 4h4q0-.425.288-.712T10 3h4q.425 0 .713.288T15 4h4q.425 0 .713.288T20 5t-.288.713T19 6v13q0 .825-.587 1.413T17 21zM17 6H7v13h10zm-7 11q.425 0 .713-.288T11 16V9q0-.425-.288-.712T10 8t-.712.288T9 9v7q0 .425.288.713T10 17m4 0q.425 0 .713-.288T15 16V9q0-.425-.288-.712T14 8t-.712.288T13 9v7q0 .425.288.713T14 17M7 6v13z" /></svg>
            </ControlButton>
        </div>
    )
}

const EditorControls = () => {
    const [state, dispatch] = useEditor();

    const undo = () => dispatch({ type: "Undo" });
    const redo = () => dispatch({ type: "Redo" });

    return (
        <div className="controls editor-controls">
            <span className="controls-header">Editor</span>
            <ControlButton onClick={undo} title="Undo" enabled={state.undoStack.length > 0}>
                <svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24">{/* Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE */}<path fill="currentColor" d="M8 19q-.425 0-.712-.288T7 18t.288-.712T8 17h6.1q1.575 0 2.738-1T18 13.5T16.838 11T14.1 10H7.8l1.9 1.9q.275.275.275.7t-.275.7t-.7.275t-.7-.275L4.7 9.7q-.15-.15-.213-.325T4.426 9t.063-.375T4.7 8.3l3.6-3.6q.275-.275.7-.275t.7.275t.275.7t-.275.7L7.8 8h6.3q2.425 0 4.163 1.575T20 13.5t-1.737 3.925T14.1 19z" /></svg>
            </ControlButton>
            <ControlButton onClick={redo} title="Redo" enabled={state.redoStack.length > 0}>
                <svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24">{/* Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE */}<path fill="currentColor" d="M9.9 19q-2.425 0-4.163-1.575T4 13.5t1.738-3.925T9.9 8h6.3l-2.6-2.6L15 4l5 5l-5 5l-1.4-1.4l2.6-2.6H9.9q-1.575 0-2.738 1T6 13.5T7.163 16T9.9 17H17v2z" /></svg>
            </ControlButton>
        </div>
    )
}

const PresetModal = ({ open, onClose }: { open: boolean; onClose: () => void }) => {
    const [state, dispatch] = useEditor();

    const applyPreset = (presetName: string) => {
        dispatch({ type: "LoadPreset", presetName });
        onClose();
    }

    const presetNames = Object.keys(PRESETS);

    return (
        <ModalInner open={open} close={onClose} title="Load preset" contents={(close) => (
            <>
                <section className="preset-options">
                    {presetNames.map(name => (
                        <button key={name} type="button" className="normal preset-option" onClick={() => applyPreset(name)}>
                            {name}
                        </button>
                    ))}
                </section>
                <div className="button-row">
                    <button type="button" className="normal secondary" onClick={close}>Cancel</button>
                </div>
            </>
        )} />
    )

}

const PresetControls = () => {
    const [state, dispatch] = useEditor();

    const [presetModalOpen, setPresetModalOpen] = useState(false);

    const openPresetModal = () =>
        setPresetModalOpen(true);

    useEffect(() => {
        if (state.pendingModal === "LoadPreset") {
            dispatch({ type: "ClearPendingModal" });
            openPresetModal();
        }
    }, [state.pendingModal]);

    return (
        <div className="controls preset-controls">
            <span className="controls-header">Presets</span>
            <ControlButton onClick={() => dispatch({ type: "OpenModal", modal: "LoadPreset" })} enabled={true} title="Load preset">
                <svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24">{/* Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE */}<path fill="currentColor" d="M12 15.575q-.2 0-.375-.062T11.3 15.3l-3.6-3.6q-.3-.3-.288-.7t.288-.7q.3-.3.713-.312t.712.287L11 12.15V5q0-.425.288-.712T12 4t.713.288T13 5v7.15l1.875-1.875q.3-.3.713-.288t.712.313q.275.3.288.7t-.288.7l-3.6 3.6q-.15.15-.325.213t-.375.062M6 20q-.825 0-1.412-.587T4 18v-2q0-.425.288-.712T5 15t.713.288T6 16v2h12v-2q0-.425.288-.712T19 15t.713.288T20 16v2q0 .825-.587 1.413T18 20z" /></svg>
            </ControlButton>
            <PresetModal open={presetModalOpen} onClose={() => setPresetModalOpen(false)} />
        </div>
    )
}

export const Controls = () => {
    const [state] = useEditor();

    const selectTarget = state.select ? getByPath(state.body, state.select) : null;
    const isRowSelected = state.select?.type === "RowHeading";
    const isColumnSelected = state.select?.type === "ColumnHeading";
    const isCellSelected = state.select?.type === "Cell";

    const controlState: ControlState = {
        selectTarget,
        isRowSelected,
        isColumnSelected,
        isCellSelected,
    };

    return (
        <div className="controls-container">
            <EditorControls />
            <RowControls {...controlState} />
            <ColumnControls {...controlState} />
            <PhonemeControls {...controlState} />
            <AnnotationControls {...controlState} />
            <PresetControls />
        </div>
    )
}