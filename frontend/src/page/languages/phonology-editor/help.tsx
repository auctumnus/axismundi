import { useState } from "react";
import { ModalInner } from "../../../components/modal/modal";

const Keybind = ({
  keys,
  description,
}: {
  keys: string[];
  description: string;
}) => {
  return (
    <>
      <dt>
        {keys.map((key, index) => (
          <kbd key={key + index}>{key}</kbd>
        ))}
      </dt>
      <dd>{description}</dd>
    </>
  );
};

export const Help = ({
  open,
  setOpen,
  editor = "phonology",
}: {
  open: boolean;
  setOpen: (open: boolean) => void;
  editor?: "phonology" | "grammar";
}) => {
  return (
    <>
      <button
        type="button"
        className="icon gray"
        onClick={() => setOpen(true)}
        aria-label="Open help modal"
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="1em"
          height="1em"
          viewBox="0 0 24 24"
        >
          {/* Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE */}
          <path
            fill="currentColor"
            d="M11.95 18q.525 0 .888-.363t.362-.887t-.362-.888t-.888-.362t-.887.363t-.363.887t.363.888t.887.362m.05 4q-2.075 0-3.9-.788t-3.175-2.137T2.788 15.9T2 12t.788-3.9t2.137-3.175T8.1 2.788T12 2t3.9.788t3.175 2.137T21.213 8.1T22 12t-.788 3.9t-2.137 3.175t-3.175 2.138T12 22m0-2q3.35 0 5.675-2.325T20 12t-2.325-5.675T12 4T6.325 6.325T4 12t2.325 5.675T12 20m.1-12.3q.625 0 1.088.4t.462 1q0 .55-.337.975t-.763.8q-.575.5-1.012 1.1t-.438 1.35q0 .35.263.588t.612.237q.375 0 .638-.25t.337-.625q.1-.525.45-.937t.75-.788q.575-.55.988-1.2t.412-1.45q0-1.275-1.037-2.087T12.1 6q-.95 0-1.812.4T8.975 7.625q-.175.3-.112.638t.337.512q.35.2.725.125t.625-.425q.275-.375.688-.575t.862-.2"
          />
        </svg>
      </button>
      <ModalInner
        open={open}
        close={() => setOpen(false)}
        title="Editor help"
        contents={(close) => (
          <>
            <div className="help-content">
              <section>
                <h3>Undo and redo</h3>
                <dl className="keybind-list">
                  <Keybind keys={["Ctrl", "z"]} description="Undo" />
                  <Keybind keys={["Ctrl", "Shift", "z"]} description="Redo" />
                  <Keybind keys={["Ctrl", "y"]} description="Redo" />
                </dl>
              </section>
              <section>
                <h3>Focus and select</h3>
                <dl className="keybind-list">
                  <Keybind
                    keys={["Tab"]}
                    description="Move focus to the next table item"
                  />
                  <Keybind
                    keys={["Shift", "Tab"]}
                    description="Move focus to the previous table item"
                  />
                  <Keybind
                    keys={["Space"]}
                    description="Toggle select focused cell"
                  />
                  <dt>
                    <span className="keybind" aria-label="Left">
                      <svg
                        xmlns="http://www.w3.org/2000/svg"
                        width="1em"
                        height="1em"
                        viewBox="0 0 24 24"
                      >
                        {/* Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE */}
                        <path
                          fill="currentColor"
                          d="m7.85 13l2.85 2.85q.3.3.288.7t-.288.7q-.3.3-.712.313t-.713-.288L4.7 12.7q-.3-.3-.3-.7t.3-.7l4.575-4.575q.3-.3.713-.287t.712.312q.275.3.288.7t-.288.7L7.85 11H19q.425 0 .713.288T20 12t-.288.713T19 13z"
                        />
                      </svg>
                    </span>
                    <span className="keybind" aria-label="Up">
                      <svg
                        xmlns="http://www.w3.org/2000/svg"
                        width="1em"
                        height="1em"
                        viewBox="0 0 24 24"
                      >
                        {/* Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE */}
                        <path
                          fill="currentColor"
                          d="m11 8.8l-2.9 2.9q-.275.275-.7.275t-.7-.275t-.275-.7t.275-.7l4.6-4.6q.3-.3.7-.3t.7.3l4.6 4.6q.275.275.275.7t-.275.7t-.7.275t-.7-.275L13 8.8V17q0 .425-.288.713T12 18t-.712-.288T11 17z"
                        />
                      </svg>
                    </span>
                    <span className="keybind" aria-label="Down">
                      <svg
                        xmlns="http://www.w3.org/2000/svg"
                        width="1em"
                        height="1em"
                        viewBox="0 0 24 24"
                      >
                        {/* Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE */}
                        <path
                          fill="currentColor"
                          d="M11 14.2V6q0-.425.288-.712T12 5t.713.288T13 6v8.2l2.9-2.9q.275-.275.7-.275t.7.275t.275.7t-.275.7l-4.6 4.6q-.3.3-.7.3t-.7-.3l-4.6-4.6q-.275-.275-.275-.7t.275-.7t.7-.275t.7.275z"
                        />
                      </svg>
                    </span>
                    <span className="keybind" aria-label="Right">
                      <svg
                        xmlns="http://www.w3.org/2000/svg"
                        width="1em"
                        height="1em"
                        viewBox="0 0 24 24"
                      >
                        {/* Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE */}
                        <path
                          fill="currentColor"
                          d="M16.15 13H5q-.425 0-.712-.288T4 12t.288-.712T5 11h11.15L13.3 8.15q-.3-.3-.288-.7t.288-.7q.3-.3.713-.312t.712.287L19.3 11.3q.15.15.213.325t.062.375t-.062.375t-.213.325l-4.575 4.575q-.3.3-.712.288t-.713-.313q-.275-.3-.288-.7t.288-.7z"
                        />
                      </svg>
                    </span>
                  </dt>
                  <dd>Move focus</dd>
                </dl>
              </section>
              <section>
                <h3>Merging cells</h3>
                <dl className="keybind-list">
                  <Keybind
                    keys={["m"]}
                    description="Merge selected cell with focused cell"
                  />
                  <Keybind keys={["Shift", "m"]} description="Unmerge cell" />
                </dl>
              </section>
              {editor === "phonology" && (
                <section>
                  <h3>Headings</h3>
                  <dl className="keybind-list">
                    <Keybind keys={["h", "e"]} description="Edit heading" />
                    <Keybind
                      keys={["h", "a"]}
                      description="Add heading before"
                    />
                    <Keybind
                      keys={["h", "A"]}
                      description="Add heading after"
                    />
                    <Keybind keys={["h", "s"]} description="Split heading" />
                    <Keybind keys={["h", "d"]} description="Delete heading" />
                  </dl>
                </section>
              )}
              {editor === "phonology" ? (
                <>
                  <section>
                    <h3>Phonemes</h3>
                    <dl className="keybind-list">
                      <Keybind keys={["p", "a"]} description="Add phoneme" />
                      <Keybind keys={["p", "e"]} description="Edit phoneme" />
                      <Keybind keys={["p", "d"]} description="Delete phoneme" />
                    </dl>
                  </section>
                  <section>
                    <h3>Annotations</h3>
                    <dl className="keybind-list">
                      <Keybind keys={["a", "a"]} description="Add annotation" />
                      <Keybind
                        keys={["a", "e"]}
                        description="Edit annotation"
                      />
                      <Keybind
                        keys={["a", "d"]}
                        description="Delete annotation"
                      />
                      <Keybind
                        keys={["a", "l"]}
                        description="Link annotation"
                      />
                    </dl>
                  </section>
                </>
              ) : (
                <>
                  <section>
                    <h3>Sound changes</h3>
                    <dl className="keybind-list">
                      <Keybind
                        keys={["Enter"]}
                        description="Edit the focused cell’s sound changes"
                      />
                    </dl>
                  </section>
                  <section>
                    <h3>Word templates</h3>
                    <p>
                      These are replaced before Lexurgy runs, in shared and cell
                      sound changes.
                    </p>
                    <dl className="keybind-list">
                      <dt>
                        <code>{"%%{word}"}</code>
                      </dt>
                      <dd>The current word’s spelling.</dd>
                      <dt>
                        <code>{"%%{ipa}"}</code>
                      </dt>
                      <dd>The current word’s stored IPA.</dd>
                      <dt>
                        <code>{"%%{extra.path}"}</code>
                      </dt>
                      <dd>
                        A value from the word’s extra data; use dots for object
                        keys and numeric array indices, such as{" "}
                        <code>{"%%{extra.stems.0}"}</code>.
                      </dd>
                    </dl>
                    <p className="hint">
                      Previews only know the example word, so only{" "}
                      <code>{"%%{word}"}</code> works there.
                    </p>
                  </section>
                </>
              )}
            </div>
            <div className="button-row">
              <button type="button" className="normal" onClick={close}>
                Close
              </button>
            </div>
          </>
        )}
      />
    </>
  );
};
