import React from "react";
import ReactDOM from "react-dom/client";
import type { GroupBase, OptionsOrGroups } from "react-select";
import { AsyncSelect } from "../async-select/async-select";

interface ScopeOptionRaw {
  id: string;
  name: string;
  abbreviation: string;
}

interface ScopeOption extends ScopeOptionRaw {
  label: string;
  value: string;
}

interface ScopeMultiselectProps {
  inputId: string;
  inputName: string;
  options: ScopeOption[];
  initialSelectedIds: string[];
  placeholder: string;
  required: boolean;
}

function ScopeMultiselect({
  inputId,
  inputName,
  options,
  initialSelectedIds,
  placeholder,
  required,
}: ScopeMultiselectProps) {
  const initialOptions = initialSelectedIds
    .map((id) => options.find((option) => option.id === id))
    .filter((option): option is ScopeOption => option !== undefined);
  const [selected, setSelected] = React.useState<readonly ScopeOption[]>(
    initialOptions,
  );

  const loadOptions = (
    inputValue: string,
  ): Promise<OptionsOrGroups<ScopeOption, GroupBase<ScopeOption>>> => {
    const query = inputValue.trim().toLowerCase();
    return Promise.resolve(
      query
        ? options.filter(
            (option) =>
              option.name.toLowerCase().includes(query) ||
              option.abbreviation.toLowerCase().includes(query),
          )
        : options,
    );
  };

  return (
    <>
      <AsyncSelect<ScopeOption, true, GroupBase<ScopeOption>>
        inputId={inputId}
        isMulti
        isClearable
        closeMenuOnSelect={false}
        cacheOptions
        defaultOptions
        loadOptions={loadOptions}
        value={selected}
        onChange={(newValue) => setSelected(newValue ?? [])}
        placeholder={placeholder}
        required={required}
      />
      {selected.map((option) => (
        <input key={option.id} type="hidden" name={inputName} value={option.id} />
      ))}
    </>
  );
}

export function mountGrammarTableScopeMultiselect(
  containerId: string,
  options: {
    inputId: string;
    inputName: string;
    allOptions: ScopeOptionRaw[];
    initialSelectedIds: string[];
    placeholder: string;
    required: boolean;
  },
) {
  const container = document.getElementById(containerId);
  if (!container) return;

  const scopeOptions = options.allOptions.map((option) => ({
    ...option,
    label: `${option.name} (${option.abbreviation})`,
    value: option.id,
  }));

  ReactDOM.createRoot(container).render(
    <ScopeMultiselect
      inputId={options.inputId}
      inputName={options.inputName}
      options={scopeOptions}
      initialSelectedIds={options.initialSelectedIds}
      placeholder={options.placeholder}
      required={options.required}
    />,
  );
}

if (typeof window !== "undefined") {
  (window as any).mountGrammarTableScopeMultiselect =
    mountGrammarTableScopeMultiselect;
}
