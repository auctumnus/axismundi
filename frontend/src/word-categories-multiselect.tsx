import React from "react";
import ReactDOM from "react-dom/client";
import { AsyncSelect } from "./components/async-select";
import type { GroupBase, OptionsOrGroups } from "react-select";

export interface CategoryOption {
  id: string;
  name: string;
  abbreviation: string;
  label: string;
  value: string;
}

interface CategoryRaw {
  id: string;
  name: string;
  abbreviation: string;
}

interface WordCategoriesMultiselectProps {
  inputId: string;
  inputName: string;
  allOptions: CategoryOption[];
  initialSelectedAbbrevs: string[];
}

function toOption(raw: CategoryRaw): CategoryOption {
  return {
    ...raw,
    label: `${raw.name} (${raw.abbreviation})`,
    value: raw.abbreviation,
  };
}

function WordCategoriesMultiselect({
  inputId,
  inputName,
  allOptions,
  initialSelectedAbbrevs,
}: WordCategoriesMultiselectProps) {
  const initialOptions = initialSelectedAbbrevs
    .map(abbr => allOptions.find(o => o.abbreviation === abbr))
    .filter((o): o is CategoryOption => o !== undefined);

  const [selected, setSelected] =
    React.useState<readonly CategoryOption[]>(initialOptions);

  const loadOptions = (
    inputValue: string,
  ): Promise<OptionsOrGroups<CategoryOption, GroupBase<CategoryOption>>> => {
    const q = inputValue.trim().toLowerCase();
    const filtered = q.length === 0
      ? allOptions
      : allOptions.filter(
        o =>
          o.name.toLowerCase().includes(q) ||
          o.abbreviation.toLowerCase().includes(q),
      );
    return Promise.resolve(filtered);
  };

  const handleChange = (
    newValue: readonly CategoryOption[] | null,
  ) => {
    setSelected(newValue ?? []);
  };

  const formatOptionLabel = (option: CategoryOption) => (
    <span className="word-category-option-content">
      {option.name}{" "}
      <span className="word-category-abbrev">({option.abbreviation})</span>
    </span>
  );

  return (
    <div className="word-categories-multiselect">
      <AsyncSelect<CategoryOption, true, GroupBase<CategoryOption>>
        inputId={inputId}
        isMulti
        cacheOptions
        defaultOptions
        loadOptions={loadOptions}
        formatOptionLabel={formatOptionLabel}
        value={selected}
        onChange={handleChange}
        isClearable
        placeholder="-- select categories --"
        loadingMessage={() => "Searching..."}
        className="word-categories-multiselect-select"
        classNamePrefix="word-categories-select"
      />
      {selected.map(option => (
        <input
          key={option.abbreviation}
          type="hidden"
          name={inputName}
          value={option.abbreviation}
        />
      ))}
    </div>
  );
}

export function mountWordCategoriesMultiselect(
  containerId: string,
  options: {
    inputId: string;
    inputName: string;
    allOptions: CategoryRaw[];
    initialSelectedAbbrevs: string[];
  },
) {
  const container = document.getElementById(containerId);
  if (!container) {
    console.error(`Container #${containerId} not found`);
    return;
  }

  // Hide the no-JS fallback <select multiple> and disable its name so it
  // doesn't double-submit alongside the React-managed hidden inputs.
  const fallback = container.querySelector("select[multiple]") as
    | HTMLSelectElement
    | null;
  if (fallback) {
    fallback.style.display = "none";
    fallback.removeAttribute("name");
  }

  const allOptions = options.allOptions.map(toOption);

  const root = ReactDOM.createRoot(container);
  root.render(
    <WordCategoriesMultiselect
      inputId={options.inputId}
      inputName={options.inputName}
      allOptions={allOptions}
      initialSelectedAbbrevs={options.initialSelectedAbbrevs}
    />,
  );
}

if (typeof window !== "undefined") {
  (window as any).mountWordCategoriesMultiselect = mountWordCategoriesMultiselect;
}
