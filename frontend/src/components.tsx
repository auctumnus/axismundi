import React from 'react'
import type { PropsWithChildren, ReactNode, Ref } from 'react'
import ReactDOM from 'react-dom'
import { Icon as IconifyIcon } from '@iconify/react/dist/offline'
import './icons' // Preload icons
import './slate-editor.css'

interface BaseProps {
  className?: string
  [key: string]: unknown
}

export const Button = React.forwardRef(
  (
    {
      className = '',
      active,
      reversed,
      ...props
    }: PropsWithChildren<
      {
        active?: boolean
        reversed?: boolean
      } & BaseProps
    >,
    ref: Ref<HTMLButtonElement>
  ) => (
    <button
      {...props}
      ref={ref}
      type="button"
      className={`editor-button ${active ? 'active' : ''} ${reversed ? 'reversed' : ''} ${className}`}
    />
  )
)

export const Icon = React.forwardRef(
  (
    { className = '', children, ...props }: PropsWithChildren<BaseProps>,
    ref: Ref<HTMLSpanElement>
  ) => {
    // Convert Material Icons name to Iconify format
    // Material Icons use underscores, Iconify uses dashes
    const iconName = typeof children === 'string'
      ? `material-symbols:${children.replace(/_/g, '-')}`
      : 'material-symbols:help'

    return (
      <span
        {...props}
        ref={ref}
        className={`editor-icon ${className}`}
      >
        <IconifyIcon icon={iconName} />
      </span>
    )
  }
)

export const Menu = React.forwardRef(
  (
    { className = '', ...props }: PropsWithChildren<BaseProps>,
    ref: Ref<HTMLDivElement>
  ) => (
    <div
      {...props}
      data-test-id="menu"
      ref={ref}
      className={`editor-menu ${className}`}
    />
  )
)

export const Portal = ({ children }: { children?: ReactNode }) => {
  return typeof document === 'object'
    ? ReactDOM.createPortal(children, document.body)
    : null
}

export const Toolbar = React.forwardRef(
  (
    { className = '', ...props }: PropsWithChildren<BaseProps>,
    ref: Ref<HTMLDivElement>
  ) => (
    <Menu
      {...props}
      ref={ref}
      className={`editor-toolbar ${className}`}
    />
  )
)

export const MentionDropdown = React.forwardRef(
  (
    { className = '', ...props }: PropsWithChildren<BaseProps>,
    ref: Ref<HTMLDivElement>
  ) => (
    <div
      {...props}
      ref={ref}
      className={`mention-dropdown ${className}`}
    />
  )
)

export const MentionItem = React.forwardRef(
  (
    {
      className = '',
      selected,
      ...props
    }: PropsWithChildren<
      {
        selected?: boolean
      } & BaseProps
    >,
    ref: Ref<HTMLDivElement>
  ) => (
    <div
      {...props}
      ref={ref}
      className={`mention-item ${selected ? 'selected' : ''} ${className}`}
    />
  )
)
