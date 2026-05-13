import React, { type ComponentProps } from "react";
import ReactDOM from "react-dom/client";
import OriginalAsyncSelect, { type AsyncProps } from "react-select/async";
import type {
  ClearIndicatorProps,
  DropdownIndicatorProps,
  GroupBase,
  NoticeProps,
  OptionsOrGroups,
  StylesConfig,
} from "react-select";

const DropdownIndicator = <
  OptionType,
  IsMulti extends boolean,
  Group extends GroupBase<OptionType>,
>({
  innerProps,
}: DropdownIndicatorProps<OptionType, IsMulti, Group>) => {
  return (
    <span className="indicator" {...innerProps}>
      <svg className="icon" aria-hidden>
        <use href="#icon-chevron-down" />
      </svg>
    </span>
  );
};

const ClearIndicator = <OptionType,>({
  innerProps,
}: ClearIndicatorProps<OptionType>) => {
  return (
    <span className="indicator" {...innerProps}>
      <svg className="icon" aria-hidden>
        <use href="#icon-close-small" />
      </svg>
    </span>
  );
};

const CrossIcon = ({}) => {
  return (
    <span className="indicator">
      <svg className="icon" aria-hidden>
        <use href="#icon-close-small" />
      </svg>
    </span>
  );
};

const NoOptionsMessage = <OptionType,>(props: NoticeProps<OptionType>) => {
  return (
    <div className="no-options">
      {props.hasValue ? "no results" : "type to search"}
    </div>
  );
};

export const AsyncSelect = <
  OptionType,
  IsMulti extends boolean,
  Group extends GroupBase<OptionType> = GroupBase<OptionType>,
>(
  props: AsyncProps<OptionType, IsMulti, Group>,
) => {
  return (
    <OriginalAsyncSelect
      {...props}
      className={props.className + " async-select"}
      classNamePrefix="async-select"
      unstyled
      menuPortalTarget={typeof document !== "undefined" ? document.body : null}
      menuPosition="fixed"
      components={{
        DropdownIndicator,
        ClearIndicator,
        NoOptionsMessage,
        CrossIcon,
        ...props.components,
      }}
    />
  );
};
