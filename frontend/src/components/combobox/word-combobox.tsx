import React from "react";
import ReactDOM from "react-dom/client";
import { AsyncSelect } from "../async-select/async-select";
import type { GroupBase, OptionsOrGroups, StylesConfig } from "react-select";

export interface WordSearchResult {
  id: string;
  word: string;
  slug: string;
  lemma: number;
  bookmark: string;
  language_code: string;
  word_class_abbreviation?: string;
  ipa?: string;
  extra?: unknown;
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

export interface WordOption extends WordSearchResult {
  label: string;
  value: string;
  language_name: string;
}

interface WordComboboxProps {
  inputId: string;
  inputName: string;
  initialValue?: string;
  initialOption?: WordOption | null;
  required?: boolean;
  excludeId?: string;
  onChange?: (selected: WordOption | null) => void;
  languageFilter?: string; // Optional filter for languages (single language code)
  initialSearch?: string; // Pre-fill the search input and preload results
}

export function WordCombobox({
  inputId,
  inputName,
  initialValue = "",
  initialOption = null,
  required = false,
  excludeId,
  onChange,
  languageFilter,
  initialSearch,
}: WordComboboxProps) {
  const [selectedBookmark, setSelectedBookmark] = React.useState(initialValue);
  const [selectedOption, setSelectedOption] = React.useState<WordOption | null>(
    initialOption,
  );
  const [preloadedOptions, setPreloadedOptions] = React.useState<
    OptionsOrGroups<WordOption, GroupBase<WordOption>> | boolean
  >(false);

  React.useEffect(() => {
    if (!initialSearch || initialSearch.length === 0) return;
    let cancelled = false;
    (async () => {
      try {
        if (languageFilter) {
          const params = new URLSearchParams({ q: initialSearch, limit: "20" });
          const response = await fetch(
            `/api/languages/${languageFilter}/words?${params}`,
          );
          if (!response.ok || cancelled) return;
          const data: { items: WordSearchResult[] } = await response.json();
          if (!cancelled) {
            setPreloadedOptions(
              data.items.map((word) => ({
                ...word,
                label: word.word,
                value: word.bookmark,
                language_name: languageFilter,
              })),
            );
          }
        }
      } catch {
        // silently fail - user can still type to search
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [initialSearch, languageFilter]);

  // Load options from API
  const loadOptions = async (
    inputValue: string,
  ): Promise<OptionsOrGroups<WordOption, GroupBase<WordOption>>> => {
    if (!inputValue || inputValue.length === 0) {
      return [];
    }

    try {
      if (languageFilter) {
        // Use single-language search endpoint
        const params = new URLSearchParams({ q: inputValue, limit: "20" });
        const response = await fetch(
          `/api/languages/${languageFilter}/words?${params}`,
        );

        if (!response.ok) {
          throw new Error("Failed to fetch results");
        }

        const data: { items: WordSearchResult[] } = await response.json();

        // Map to options format
        const options = data.items.map((word) => ({
          ...word,
          label: word.word,
          value: word.bookmark,
          language_name: languageFilter, // language name is not needed when filtering by single language
        }));

        return options;
      } else {
        // use cross-language search endpoint
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
      }
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
        {option.lemma !== 1 && (
          <span className="word-combobox-lemma">/{option.lemma}</span>
        )}
        {option.word_class_abbreviation && (
          <span className="word-combobox-class">
            {" "}
            (
            <span className="word-class-abbreviation">
              {option.word_class_abbreviation}
            </span>
            )
          </span>
        )}
      </span>
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
    if (onChange) {
      onChange(newValue);
    }
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
        isClearable
        placeholder="-- select a word --"
        loadingMessage={() => "Searching..."}
        className="word-combobox-select"
        classNamePrefix="word-select"
        defaultInputValue={initialSearch ?? undefined}
        defaultOptions={preloadedOptions}
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
