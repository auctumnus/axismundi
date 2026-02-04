import React from "react";
import ReactDOM from "react-dom/client";
import AsyncSelect from "react-select/async";
import type { GroupBase, OptionsOrGroups, StylesConfig } from "react-select";
import "./word-combobox.css";

interface WordSearchResult {
  id: string;
  word: string;
  slug: string;
  lemma: number;
  bookmark: string;
  language_code: string;
  word_class_abbreviation?: string;
  ipa?: string;
}

interface LanguageWordsGroup {
  language_id: string;
  language_code: string;
  language_name: string;
  words: WordSearchResult[];
}

interface CrossLanguageSearchResponse {
  languages: LanguageWordsGroup[];
}

interface WordOption extends WordSearchResult {
  label: string;
  value: string;
  language_name: string;
}

interface WordComboboxProps {
  inputId: string;
  inputName: string;
  placeholder?: string;
  initialValue?: string;
  required?: boolean;
  excludeId?: string;
}

function WordCombobox({
  inputId,
  inputName,
  placeholder = "",
  initialValue = "",
  required = false,
  excludeId,
}: WordComboboxProps) {
  const [selectedBookmark, setSelectedBookmark] = React.useState(initialValue);
  const [selectedOption, setSelectedOption] = React.useState<WordOption | null>(
    null,
  );

  // Load options from API
  const loadOptions = async (
    inputValue: string,
  ): Promise<OptionsOrGroups<WordOption, GroupBase<WordOption>>> => {
    if (!inputValue || inputValue.length === 0) {
      return [];
    }

    try {
      const params = new URLSearchParams({ q: inputValue, limit: "20" });
      if (excludeId) {
        params.append("exclude_id", excludeId);
      }

      const response = await fetch(`/api/words/search?${params}`);

      if (!response.ok) {
        throw new Error("Failed to fetch results");
      }

      const data: CrossLanguageSearchResponse = await response.json();

      // Convert to grouped options format
      const groupedOptions = data.languages.map((lang) => ({
        label: lang.language_name,
        options: lang.words.map((word) => ({
          ...word,
          label: word.word,
          value: word.bookmark,
          language_name: lang.language_name,
        })),
      }));

      return groupedOptions;
    } catch (error) {
      console.error("Failed to fetch word search results:", error);
      return [];
    }
  };

  // Custom option label formatter
  const formatOptionLabel = (option: WordOption) => (
    <div className="word-combobox-option-content">
      <span className="word-combobox-word">
        {option.word}
        {option.word_class_abbreviation && (
          <span className="word-combobox-class">
            {" "}
            ({option.word_class_abbreviation})
          </span>
        )}
      </span>
      {option.ipa && <span className="word-combobox-ipa"> /{option.ipa}/</span>}
    </div>
  );

  // Custom format for group labels
  const formatGroupLabel = (group: GroupBase<WordOption>) => (
    <div className="word-combobox-group-label">{group.label}</div>
  );

  // Handle selection
  const handleChange = (newValue: WordOption | null) => {
    setSelectedOption(newValue);
    setSelectedBookmark(newValue?.bookmark || "");
  };

  // Custom styles matching the existing form select styles
  const customStyles: StylesConfig<WordOption, false, GroupBase<WordOption>> = {
    control: (base, state) => ({
      ...base,
      fontFamily: "var(--font-normal)",
      padding: "0",
      //border: `1px solid var(--input-border)`,
      borderRadius: "var(--rounding)",
      backgroundColor: "var(--input-background)",
      color: "var(--foreground-primary)",
      fontSize: "1rem",
      minHeight: "40px",
      outline: "0",
      boxShadow: state.isFocused ? "0 0 0 2px var(--focus-ring)" : "none",
      borderColor: state.isFocused
        ? "var(--input-border-focus)"
        : "var(--input-border)",
      transition:
        "background-color 250ms ease-in-out, border-color 250ms ease-in-out",
      "&:hover": {
        borderColor: state.isFocused
          ? "var(--input-border-focus)"
          : "var(--input-border)",
      },
    }),
    valueContainer: (base) => ({
      ...base,
      padding: "0 0.5rem",
    }),
    input: (base) => ({
      ...base,
      color: "var(--foreground-primary)",
      margin: "0",
      padding: "0",
    }),
    placeholder: (base) => ({
      ...base,
      color: "var(--foreground-secondary)",
    }),
    singleValue: (base) => ({
      ...base,
      color: "var(--foreground-primary)",
    }),
    menu: (base) => ({
      ...base,
      backgroundColor: "var(--background-panel)",
      border: `1px solid var(--input-border)`,
      borderRadius: "var(--rounding)",
      boxShadow: "0 4px 12px rgba(0, 0, 0, 0.15)",
      zIndex: 9999,
    }),
    menuList: (base) => ({
      ...base,
      padding: "0",
    }),
    option: (base, state) => ({
      ...base,
      backgroundColor: state.isFocused
        ? "var(--input-background-focus)"
        : state.isSelected
          ? "var(--input-background-focus)"
          : "transparent",
      color: "var(--foreground-primary)",
      cursor: "pointer",
      padding: "8px 12px",
      transition: "background-color 150ms ease-in-out",
      "&:active": {
        backgroundColor: "var(--input-background-focus)",
      },
    }),
    group: (base) => ({
      ...base,
      paddingTop: 0,
      paddingBottom: 0,
    }),
    groupHeading: (base) => ({
      ...base,
      fontFamily: "var(--font-heading)",
      fontSize: "0.75rem",
      fontWeight: "bold",
      color: "var(--foreground-secondary)",
      textTransform: "uppercase",
      padding: "8px 12px",
      backgroundColor: "var(--background-panel)",
      borderTop: `1px solid var(--input-border)`,
      marginTop: 0,
      marginBottom: 0,
      position: "sticky",
      top: 0,
      zIndex: 1,
    }),
    indicatorSeparator: (base) => ({
      ...base,
      backgroundColor: "var(--input-border)",
    }),
    dropdownIndicator: (base) => ({
      ...base,
      color: "var(--foreground-secondary)",
      transition: "color 150ms ease-in-out",
      "&:hover": {
        color: "var(--foreground-primary)",
      },
    }),
    clearIndicator: (base) => ({
      ...base,
      color: "var(--foreground-secondary)",
      transition: "color 150ms ease-in-out",
      "&:hover": {
        color: "var(--foreground-primary)",
      },
    }),
    loadingIndicator: (base) => ({
      ...base,
      color: "var(--foreground-secondary)",
    }),
    noOptionsMessage: (base) => ({
      ...base,
      color: "var(--foreground-secondary)",
    }),
    loadingMessage: (base) => ({
      ...base,
      color: "var(--foreground-secondary)",
    }),
  };

  return (
    <div className="word-combobox">
      <AsyncSelect<WordOption, false, GroupBase<WordOption>>
        inputId={inputId}
        cacheOptions
        loadOptions={loadOptions}
        formatOptionLabel={formatOptionLabel}
        formatGroupLabel={formatGroupLabel}
        onChange={handleChange}
        value={selectedOption}
        placeholder={placeholder}
        isClearable
        styles={customStyles}
        noOptionsMessage={({ inputValue }) =>
          inputValue ? "No words found" : "Type to search..."
        }
        loadingMessage={() => "Searching..."}
        className="word-combobox-select"
        classNamePrefix="word-select"
      />
      <input
        type="hidden"
        name={inputName}
        value={selectedBookmark}
        required={required}
      />
    </div>
  );
}

// Mount function
export function mountWordCombobox(
  containerId: string,
  options: {
    inputId: string;
    inputName: string;
    placeholder?: string;
    initialValue?: string;
    required?: boolean;
    excludeId?: string;
  },
) {
  const container = document.getElementById(containerId);
  if (!container) {
    console.error(`Container #${containerId} not found`);
    return;
  }

  const root = ReactDOM.createRoot(container);
  root.render(
    <WordCombobox
      inputId={options.inputId}
      inputName={options.inputName}
      placeholder={options.placeholder}
      initialValue={options.initialValue}
      required={options.required}
      excludeId={options.excludeId}
    />,
  );
}

// Make it available globally for HTML templates
if (typeof window !== "undefined") {
  (window as any).mountWordCombobox = mountWordCombobox;
}
