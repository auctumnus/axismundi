import React from "react";
import ReactDOM from "react-dom/client";
import type { GroupBase, OptionsOrGroups, StylesConfig } from "react-select";
import { AsyncSelect } from "../async-select/async-select";

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
