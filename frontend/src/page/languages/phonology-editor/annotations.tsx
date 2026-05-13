import { useState } from "react";
import {
  SteppedModal,
  type SteppedModalStep,
} from "../../../components/modal/stepped-modal";
import { useEditor } from "./state";
import { getByPath } from "./path";
import { Tooltip } from "../../../components/tooltip/tooltip";

export const Annotations = () => {
  const [state, dispatch] = useEditor();

  // modal state
  const [modalOpen, setModalOpen] = useState(false);
  const [modalStep, setModalStep] = useState(0);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [editText, setEditText] = useState("");

  const closeModal = () => {
    setModalOpen(false);
    setModalStep(0);
  };

  const handleAnnotationClick = (index: number) => {
    setSelectedIndex(index);
    setModalStep(0);
    setModalOpen(true);
  };

  const handleChooseEdit = () => {
    setEditText(state.body.annotations[selectedIndex]!);
    setModalStep(1);
  };

  const handleChooseDelete = () => {
    dispatch({
      type: "DeleteAnnotationEntirely",
      annotationIndex: selectedIndex,
    });
    closeModal();
  };

  const handleEditSave = () => {
    dispatch({
      type: "EditAnnotation",
      annotationIndex: selectedIndex,
      newText: editText,
    });
    closeModal();
  };

  const chooseStep: SteppedModalStep = {
    title: `annotation ${selectedIndex + 1}`,
    content: (close) => (
      <>
        <section className="phoneme-options">
          <button
            type="button"
            className="normal phoneme-option"
            onClick={handleChooseEdit}
          >
            edit text
          </button>
          <button
            type="button"
            className="normal phoneme-option danger"
            onClick={handleChooseDelete}
          >
            delete annotation
          </button>
        </section>
        <div className="button-row">
          <button type="button" className="normal secondary" onClick={close}>
            Cancel
          </button>
        </div>
      </>
    ),
  };

  const editStep: SteppedModalStep = {
    title: "edit annotation",
    content: (close) => (
      <>
        <section>
          <label htmlFor="annotation-text">Annotation</label>
          <input
            name="annotation-text"
            className="normal"
            type="text"
            value={editText}
            onChange={(e) => setEditText(e.target.value)}
            autoFocus
            onKeyDown={(e) =>
              e.key === "Enter" && editText.trim() && handleEditSave()
            }
          />
        </section>
        <div className="button-row">
          <button type="button" className="normal secondary" onClick={close}>
            Cancel
          </button>
          <button
            type="button"
            className="normal"
            onClick={handleEditSave}
            disabled={!editText.trim()}
          >
            Save
          </button>
        </div>
      </>
    ),
  };

  const focusedAnnotations =
    state.focus?.type === "Cell"
      ? (getByPath(state.body, state.focus)
          ?.phonemes.map((p) => p.annotations)
          .flat() ?? [])
      : [];

  const annotations = state.body.annotations.map((text, index) => {
    return (
      <li
        key={index}
        className={focusedAnnotations.includes(index) ? "focused" : ""}
      >
        <Tooltip content="Edit annotation">
          <button
            type="button"
            className="annotation-link"
            onClick={() => handleAnnotationClick(index)}
          >
            {text}
          </button>
        </Tooltip>
      </li>
    );
  });

  const listClassName =
    "annotations" +
    (focusedAnnotations.length > 0 ? " has-focused" : "") +
    (annotations.length === 0 ? " empty" : "");

  return (
    <>
      {state.body.annotations.length ? (
        <h3 className="annotations-header">Annotations</h3>
      ) : null}
      <ol className={listClassName}>{annotations}</ol>
      <SteppedModal
        open={modalOpen}
        close={closeModal}
        step={modalStep}
        steps={[chooseStep, editStep]}
      />
    </>
  );
};
