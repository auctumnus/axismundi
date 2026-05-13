import { Fragment, useRef, useState, useSyncExternalStore } from "react";
import { createRoot } from "react-dom/client";
import { ModalInner } from "../../../components/modal/modal";
import type {
  Request,
  Response,
  Error as LexurgyError,
} from "./sound-change-runner";
import {
  ComboboxInput,
  ComboboxOption,
  ComboboxOptions,
  Combobox as ComboboxOuter,
  Description,
  Field,
  Label,
} from "@headlessui/react";

const SaveButton = ({
  languageCode,
  setId,
  getChanges,
}: {
  languageCode: string;
  setId: string;
  getChanges: () => string;
}) => {
  const [saving, setSaving] = useState(false);
  const [errorModalOpen, setErrorModalOpen] = useState(false);
  const [errorMessage, setErrorMessage] = useState("");

  const handleClick = async () => {
    console.log("meow");
    setSaving(true);
    const changes = getChanges();
    try {
      console.log("meow 2");
      await Promise.all([
        (async () => {
          const r = await fetch(
            `/api/languages/${languageCode}/sound-change-sets/${setId}`,
            {
              method: "PUT",
              headers: {
                "Content-Type": "application/json",
              },
              body: JSON.stringify({ changes }),
            },
          );

          if (!r.ok) {
            console.log("meow 4", r);
            throw new Error(`Failed to save sound changes: ${r.statusText}`);
          }

          console.log("meow 3", r);

          return r;
        })(),
        new Promise((resolve) => setTimeout(resolve, 500)), // ensure the saving state is visible for at least 500ms
      ]);
      setSaving(false);
    } catch (error) {
      console.error("Error saving sound changes:", error);
      if (error instanceof Error) {
        if (
          error.name === "NetworkError" ||
          error.message.includes("NetworkError")
        ) {
          setErrorMessage(
            "There was a network error. Either the server is down, or your internet connection is not working.",
          );
        } else {
          setErrorMessage(error.message);
        }
      } else {
        setErrorMessage("An unknown error occurred.");
      }
      setErrorModalOpen(true);
      setSaving(false);
    }
  };

  return (
    <>
      <button
        id="save-button"
        type="button"
        className="normal"
        data-language-code={languageCode}
        data-set-id={setId}
        onClick={handleClick}
        disabled={saving}
      >
        {saving ? "Saving..." : "Save"}
      </button>
      <ModalInner
        title="Failed to save changes"
        open={errorModalOpen}
        close={() => setErrorModalOpen(false)}
        contents={(close) => (
          <>
            <p>{errorMessage}</p>
            <div className="button-row">
              <button onClick={close} type="button" className="normal">
                Close
              </button>
            </div>
          </>
        )}
      />
    </>
  );
};

export const mountSaveButton = (
  container: HTMLElement,
  languageCode: string,
  setId: string,
  getChanges: () => string,
) => {
  const root = createRoot(container);
  root.render(
    <SaveButton
      languageCode={languageCode}
      setId={setId}
      getChanges={getChanges}
    />,
  );
};

const RunButton = ({
  getRequest,
  onResponse,
  onError,
}: {
  getRequest: () => Request;
  onResponse: (request: Request, response: Response) => any;
  onError: (error: LexurgyError | string) => any;
}) => {
  const [running, setRunning] = useState(false);

  const handleClick = async () => {
    setRunning(true);
    try {
      const request = getRequest();
      console.log(request);
      await Promise.all([
        (async () => {
          const response = await fetch(`/api/sound-change-sets/run`, {
            method: "POST",
            headers: {
              "Content-Type": "application/json",
            },
            body: JSON.stringify(request),
          });

          if (!response.ok) {
            const errorText = await response.text();
            try {
              const errorData = JSON.parse(errorText);
              onError(errorData.extra);
              return;
            } catch (e) {
              onError(errorText);
              return;
            }
          }

          const responseData = await response.json();
          onResponse(request, responseData as Response);
        })(),
        new Promise((resolve) => setTimeout(resolve, 500)), // ensure the running state is visible for at least 500ms
      ]);
    } catch (error) {
      console.error("Error running sound changes:", error);
      if (error instanceof Error) {
        if (
          error.name === "NetworkError" ||
          error.message.includes("NetworkError")
        ) {
          onError(
            "There was a network error. Either the server is down, or your internet connection is not working.",
          );
        } else {
          onError(error.message);
        }
      } else {
        onError("An unknown error occurred.");
      }
    } finally {
      setRunning(false);
    }
  };

  return (
    <button
      id="run-button"
      type="button"
      className="normal"
      onClick={handleClick}
      disabled={running}
    >
      {running ? "Running..." : "Run"}
    </button>
  );
};

export const mountRunButton = (
  container: HTMLElement,
  getRequest: () => Request,
  onResponse: (request: Request, response: Response) => any,
  onError: (error: LexurgyError | string) => any,
) => {
  const root = createRoot(container);
  root.render(
    <RunButton
      getRequest={getRequest}
      onResponse={onResponse}
      onError={onError}
    />,
  );
};

interface ComboboxProps {
  rulesStore: {
    subscribe: (callback: (rules: string[]) => any) => () => any;
    getSnapshot: () => string[];
  };
  multiple: boolean;
  title: string;
  description?: string;
  name: string;
}

const Checkmark = ({ className }: { className?: string }) => (
  <svg
    className={className}
    xmlns="http://www.w3.org/2000/svg"
    width="1em"
    height="1em"
    viewBox="0 0 24 24"
  >
    {/* Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE */}
    <path
      fill="currentColor"
      d="m9.55 15.15l8.475-8.475q.3-.3.7-.3t.7.3t.3.713t-.3.712l-9.175 9.2q-.3.3-.7.3t-.7-.3L4.55 13q-.3-.3-.288-.712t.313-.713t.713-.3t.712.3z"
    />
  </svg>
);

const Combobox = ({
  rulesStore,
  multiple,
  title,
  description,
  name,
}: ComboboxProps) => {
  const rules = useSyncExternalStore(
    rulesStore.subscribe,
    rulesStore.getSnapshot,
  );

  const [inputValue, setInputValue] = useState("");
  const [additionalOptions, setAdditionalOptions] = useState<string[]>([]);
  const [selectedOptions, setSelectedOptions] = useState(
    multiple ? ([] as string[]) : "",
  );

  const optionsSet = new Set([
    ...rules,
    ...additionalOptions,
    ...(multiple ? (selectedOptions as string[]) : [selectedOptions as string]),
  ]);

  const options = Array.from(optionsSet);

  if (!optionsSet.has(inputValue)) {
    options.unshift(inputValue);
  }

  const filteredOptions = options.filter(
    (option) =>
      option.toLowerCase().includes(inputValue.toLowerCase()) &&
      option.length > 0,
  );

  const interceptedOnChange = (value: string | string[] | null) => {
    let removedOptions: string[] = [];
    if (multiple) {
      removedOptions = (selectedOptions as string[]).filter(
        (option) => !(value as string[]).includes(option),
      );
    } else {
      if (selectedOptions && value !== selectedOptions) {
        removedOptions = [selectedOptions as string];
      }
    }

    removedOptions.forEach((option) => {
      if (!optionsSet.has(option)) {
        setAdditionalOptions((prev) => prev.filter((o) => o !== option));
      }
    });

    if (value === null) {
      setSelectedOptions(multiple ? [] : "");
    } else {
      if (multiple) {
        const newOptions = (value as string[]).filter(
          (option) => !optionsSet.has(option),
        );
        if (newOptions.length > 0) {
          setAdditionalOptions((prev) => [...prev, ...newOptions]);
        }
        setSelectedOptions(value as string[]);
      } else {
        if (!optionsSet.has(value as string)) {
          setAdditionalOptions((prev) => [...prev, value as string]);
        }
        setSelectedOptions(value as string);
      }
    }
  };

  const removeOption = (option: string) => {
    setAdditionalOptions((prev) => prev.filter((o) => o !== option));
    if (multiple) {
      setSelectedOptions(
        (selectedOptions as string[]).filter((o) => o !== option),
      );
    } else {
      setSelectedOptions("");
    }
  };

  const isSelected = (option: string) => {
    if (multiple) {
      return (selectedOptions as string[]).includes(option);
    } else {
      return selectedOptions === option;
    }
  };

  const input = (() => {
    if (multiple) {
      const inputRef = useRef<HTMLInputElement>(null);
      return (
        <div
          className="combobox-input-container multiple"
          onClick={() => inputRef.current?.focus()}
        >
          {(selectedOptions as string[]).map((option) => (
            <button
              type="button"
              key={option}
              className="selected-option"
              onClick={(e) => {
                e.stopPropagation();
                removeOption(option);
              }}
            >
              <span>{option}</span>
              <svg
                xmlns="http://www.w3.org/2000/svg"
                width="1em"
                height="1em"
                viewBox="0 0 24 24"
              >
                {/* Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE */}
                <path
                  fill="currentColor"
                  d="m12 13.4l-2.917 2.925q-.277.275-.704.275t-.704-.275q-.275-.275-.275-.7t.275-.7L10.6 12L7.675 9.108Q7.4 8.831 7.4 8.404t.275-.704q.275-.275.7-.275t.7.275L12 10.625L14.892 7.7q.277-.275.704-.275t.704.275q.3.3.3.713t-.3.687L13.375 12l2.925 2.917q.275.277.275.704t-.275.704q-.3.3-.712.3t-.688-.3z"
                />
              </svg>
            </button>
          ))}
          <ComboboxInput
            ref={inputRef}
            className="combobox-input multiple no-default-styles"
            aria-label={title}
            name={name}
            id={name}
            onChange={(event) => setInputValue(event.target.value)}
          />
        </div>
      );
    } else {
      return (
        <ComboboxInput
          className="combobox-input single"
          aria-label={title}
          name={name}
          id={name}
          onChange={(event) => setInputValue(event.target.value)}
        />
      );
    }
  })();

  return (
    <Field
      as="section"
      className={`combobox-field ${multiple ? "multiple" : "single"}`}
    >
      <Label>{title}</Label>
      <ComboboxOuter
        multiple={multiple}
        value={selectedOptions}
        onChange={interceptedOnChange}
      >
        <input
          type="hidden"
          value={
            multiple
              ? (selectedOptions as string[]).join("\n")
              : (selectedOptions as string)
          }
          readOnly
        />
        {input}
        {filteredOptions.length > 0 && (
          <ComboboxOptions
            className={`combobox-options ${multiple ? "multiple" : "single"}`}
            transition
            anchor={{ to: "bottom", gap: 4 }}
          >
            {filteredOptions.map((option) => (
              <ComboboxOption
                className="combobox-option"
                key={option}
                value={option}
              >
                <span className="checkmark">
                  <Checkmark className={isSelected(option) ? "visible" : ""} />
                </span>
                <span>{option}</span>
              </ComboboxOption>
            ))}
          </ComboboxOptions>
        )}
      </ComboboxOuter>
      {description ? (
        <Description as="span" className="hint">
          {description}
        </Description>
      ) : null}
    </Field>
  );
};

export const mountCombobox = (
  container: HTMLElement,
  options: ComboboxProps,
) => {
  const root = createRoot(container);
  root.render(<Combobox {...options} />);
};
