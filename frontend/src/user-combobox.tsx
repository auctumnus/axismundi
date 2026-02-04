import React from "react";
import ReactDOM from "react-dom/client";
import AsyncSelect from "react-select/async";
import type { GroupBase, OptionsOrGroups, StylesConfig } from "react-select";
import "./word-combobox.css"; // Reuse the same styles

interface UserSearchResult {
  id: string;
  username: string;
  display_name?: string;
  email?: string;
}

interface UserOption extends UserSearchResult {
  label: string;
  value: string;
}

interface UserComboboxProps {
  inputId: string;
  inputName: string;
  placeholder?: string;
  initialValue?: string;
  required?: boolean;
}

function UserCombobox({
  inputId,
  inputName,
  placeholder = "",
  initialValue = "",
  required = false,
}: UserComboboxProps) {
  const [selectedUsername, setSelectedUsername] = React.useState(initialValue);
  const [selectedOption, setSelectedOption] = React.useState<UserOption | null>(
    null,
  );

  // Load options from API
  const loadOptions = async (
    inputValue: string,
  ): Promise<OptionsOrGroups<UserOption, GroupBase<UserOption>>> => {
    if (!inputValue || inputValue.length === 0) {
      return [];
    }

    try {
      const params = new URLSearchParams({
        text_query: inputValue,
        limit: "20",
      });
      const response = await fetch(`/api/users?${params}`);

      if (!response.ok) {
        throw new Error("Failed to fetch results");
      }

      const data = await response.json();

      // Convert to options format
      const options = data.items.map((user: UserSearchResult) => ({
        ...user,
        label: user.username,
        value: user.username,
      }));

      return options;
    } catch (error) {
      console.error("Failed to fetch user search results:", error);
      return [];
    }
  };

  // Custom option label formatter
  const formatOptionLabel = (option: UserOption) => (
    <div className="word-combobox-option-content">
      <span className="word-combobox-word">
        {option.username}
        {option.display_name && (
          <span className="word-combobox-class"> ({option.display_name})</span>
        )}
      </span>
    </div>
  );

  // Handle selection
  const handleChange = (newValue: UserOption | null) => {
    setSelectedOption(newValue);
    setSelectedUsername(newValue?.username || "");
  };

  // Custom styles matching the existing form select styles
  const customStyles: StylesConfig<UserOption, false, GroupBase<UserOption>> = {
    control: (base, state) => ({
      ...base,
      fontFamily: "var(--font-normal)",
      padding: "0",
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
      <AsyncSelect<UserOption, false, GroupBase<UserOption>>
        inputId={inputId}
        cacheOptions
        loadOptions={loadOptions}
        formatOptionLabel={formatOptionLabel}
        onChange={handleChange}
        value={selectedOption}
        placeholder={placeholder}
        isClearable
        styles={customStyles}
        noOptionsMessage={({ inputValue }) =>
          inputValue ? "No users found" : "Type to search..."
        }
        loadingMessage={() => "Searching..."}
        className="word-combobox-select"
        classNamePrefix="word-select"
      />
      <input
        type="hidden"
        name={inputName}
        value={selectedUsername}
        required={required}
      />
    </div>
  );
}

// Mount function
export function mountUserCombobox(
  containerId: string,
  options: {
    inputId: string;
    inputName: string;
    placeholder?: string;
    initialValue?: string;
    required?: boolean;
  },
) {
  const container = document.getElementById(containerId);
  if (!container) {
    console.error(`Container #${containerId} not found`);
    return;
  }

  const root = ReactDOM.createRoot(container);
  root.render(
    <UserCombobox
      inputId={options.inputId}
      inputName={options.inputName}
      placeholder={options.placeholder}
      initialValue={options.initialValue}
      required={options.required}
    />,
  );
}

// Make it available globally for HTML templates
if (typeof window !== "undefined") {
  (window as any).mountUserCombobox = mountUserCombobox;
}
