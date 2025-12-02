import isHotkey from 'is-hotkey'
import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { KeyboardEvent, PointerEvent } from 'react'
import type { Descendant } from 'slate'
import {
  Editor,
  Element as SlateElement,
  Range,
  Transforms,
  createEditor,
} from 'slate'
import { withHistory } from 'slate-history'
import type { RenderElementProps, RenderLeafProps } from 'slate-react'
import {
  Editable,
  ReactEditor,
  Slate,
  useSlate,
  withReact,
} from 'slate-react'
import { Button, Icon, Toolbar, Portal, MentionDropdown, MentionItem } from './components'
import type {
  CustomEditor,
  CustomElement,
  CustomElementType,
  CustomElementWithAlign,
  CustomTextKey,
  MentionElement,
} from './custom-types'
import './slate-editor.css'

// API Types
interface ApiUser {
  username: string
  display_name: string | null
  description: string | null
  pronouns: string | null
  gender: string | null
  bookmark: string
  profile_picture_url: string | null
  created_at: string
  updated_at: string
}

interface PaginatedResponse<T> {
  items: T[]
  total: number
  has_more: boolean
}

const HOTKEYS: Record<string, CustomTextKey> = {
  'mod+b': 'bold',
  'mod+i': 'italic',
  'mod+u': 'underline',
  'mod+`': 'code',
}

const LIST_TYPES = ['numbered-list', 'bulleted-list'] as const
const TEXT_ALIGN_TYPES = ['left', 'center', 'right', 'justify'] as const

type AlignType = (typeof TEXT_ALIGN_TYPES)[number]
type ListType = (typeof LIST_TYPES)[number]
type CustomElementFormat = CustomElementType | AlignType | ListType

// Fetch users from API
async function fetchUsers(query: string): Promise<ApiUser[]> {
  try {
    const params = new URLSearchParams()
    if (query) {
      params.set('text_query', query)
    }
    params.set('limit', '10')

    const response = await fetch(`/api/users?${params}`)
    if (!response.ok) {
      console.error('Failed to fetch users:', response.statusText)
      return []
    }

    const data: PaginatedResponse<ApiUser> = await response.json()
    return data.items
  } catch (error) {
    console.error('Error fetching users:', error)
    return []
  }
}

interface RichTextEditorProps {
  initialValue?: Descendant[]
}

const RichTextExample = React.forwardRef<CustomEditor, RichTextEditorProps>(
  ({ initialValue }, forwardedRef) => {
    const [target, setTarget] = useState<Range | undefined>()
    const [index, setIndex] = useState(0)
    const [search, setSearch] = useState('')
    const [users, setUsers] = useState<ApiUser[]>([])
    const [isLoadingUsers, setIsLoadingUsers] = useState(false)
    const renderElement = useCallback(
      (props: RenderElementProps) => <Element {...props} />,
      []
    )
    const renderLeaf = useCallback(
      (props: RenderLeafProps) => <Leaf {...props} />,
      []
    )
    const editor = useMemo(() => withMentions(withHistory(withReact(createEditor()))), [])
    const ref = useRef<HTMLDivElement>(null)

    // Expose editor instance via ref
    React.useImperativeHandle(forwardedRef, () => editor, [editor])

  // Fetch users when search changes or when mention dropdown opens
  useEffect(() => {
    if (!target) {
      return
    }

    let cancelled = false
    const controller = new AbortController()

    async function loadUsers() {
      setIsLoadingUsers(true)
      const result = await fetchUsers(search)
      if (!cancelled) {
        setUsers(result)
        setIsLoadingUsers(false)
      }
    }

    // Debounce the API call
    const timeoutId = setTimeout(loadUsers, 200)

    return () => {
      cancelled = true
      controller.abort()
      clearTimeout(timeoutId)
    }
  }, [search, target])

  const onKeyDown = useCallback(
    (event: KeyboardEvent<HTMLDivElement>) => {
      // Handle mention dropdown navigation
      if (target && users.length > 0 && !isLoadingUsers) {
        switch (event.key) {
          case 'ArrowDown':
            event.preventDefault()
            setIndex(prevIndex => (prevIndex >= users.length - 1 ? 0 : prevIndex + 1))
            return
          case 'ArrowUp':
            event.preventDefault()
            setIndex(prevIndex => (prevIndex <= 0 ? users.length - 1 : prevIndex - 1))
            return
          case 'Tab':
          case 'Enter':
            event.preventDefault()
            const selectedUser = users[index]
            if (selectedUser) {
              Transforms.select(editor, target)
              insertMention(editor, selectedUser)
              setTarget(undefined)
            }
            return
          case 'Escape':
            event.preventDefault()
            setTarget(undefined)
            return
        }
      }

      // Handle hotkeys
      for (const hotkey in HOTKEYS) {
        if (isHotkey(hotkey, event as any)) {
          event.preventDefault()
          const mark = HOTKEYS[hotkey]
          toggleMark(editor, mark)
        }
      }
    },
    [editor, index, search, target, users, isLoadingUsers]
  )

  useEffect(() => {
    if (target && users.length > 0) {
      const el = ref.current
      const domRange = ReactEditor.toDOMRange(editor, target)
      const rect = domRange.getBoundingClientRect()
      if (el) {
        el.style.top = `${rect.top + window.scrollY + 24}px`
        el.style.left = `${rect.left + window.scrollX}px`
      }
    }
  }, [editor, index, search, target, users.length])

  useEffect(() => {
    const { selection } = editor

    if (selection && Range.isCollapsed(selection)) {
      const [start] = Range.edges(selection)
      const wordBefore = Editor.before(editor, start, { unit: 'word' })
      const before = wordBefore && Editor.before(editor, wordBefore)
      const beforeRange = before && Editor.range(editor, before, start)
      const beforeText = beforeRange && Editor.string(editor, beforeRange)
      const beforeMatch = beforeText && beforeText.match(/^@(\w*)$/)
      const after = Editor.after(editor, start)
      const afterRange = Editor.range(editor, start, after)
      const afterText = Editor.string(editor, afterRange)
      const afterMatch = afterText.match(/^(\s|$)/)

      if (beforeMatch && afterMatch) {
        setTarget(beforeRange)
        setSearch(beforeMatch[1] || '')
        setIndex(0)
        return
      }
    }

    setTarget(undefined)
  }, [editor, editor.selection])

  return (
    <div className="editor-container">
      <Slate editor={editor} initialValue={initialValue || defaultInitialValue}>
        <Toolbar>
          <MarkButton format="bold" icon="format_bold" />
          <MarkButton format="italic" icon="format_italic" />
          <MarkButton format="underline" icon="format_underlined" />
          <MarkButton format="code" icon="code" />
          <BlockButton format="heading-one" icon="looks_one" />
          <BlockButton format="heading-two" icon="looks_two" />
          <BlockButton format="block-quote" icon="format_quote" />
          <BlockButton format="numbered-list" icon="format_list_numbered" />
          <BlockButton format="bulleted-list" icon="format_list_bulleted" />
          <BlockButton format="left" icon="format_align_left" />
          <BlockButton format="center" icon="format_align_center" />
          <BlockButton format="right" icon="format_align_right" />
          <BlockButton format="justify" icon="format_align_justify" />
        </Toolbar>
        <Editable
          renderElement={renderElement}
          renderLeaf={renderLeaf}
          spellCheck
          autoFocus
          onKeyDown={onKeyDown}
        />
        {target && (
          <Portal>
            <MentionDropdown ref={ref}>
              {isLoadingUsers ? (
                <div style={{ padding: '8px 12px', color: '#666' }}>Loading...</div>
              ) : users.length > 0 ? (
                users.map((user, i) => {
                  const displayName = user.display_name || user.username
                  return (
                    <MentionItem
                      key={user.bookmark}
                      selected={i === index}
                      onClick={() => {
                        Transforms.select(editor, target)
                        insertMention(editor, user)
                        setTarget(undefined)
                      }}
                    >
                      <div>
                        <strong>@{user.username}</strong>
                        {user.display_name && (
                          <span style={{ marginLeft: '8px', color: '#666' }}>
                            {user.display_name}
                          </span>
                        )}
                      </div>
                    </MentionItem>
                  )
                })
              ) : (
                <div style={{ padding: '8px 12px', color: '#666' }}>No users found</div>
              )}
            </MentionDropdown>
          </Portal>
        )}
      </Slate>
    </div>
  )
})

RichTextExample.displayName = 'RichTextExample'

const withMentions = (editor: CustomEditor): CustomEditor => {
  const { isInline, isVoid, markableVoid } = editor

  editor.isInline = (element) => {
    return (element as CustomElement).type === 'mention' ? true : isInline(element)
  }

  editor.isVoid = (element) => {
    return (element as CustomElement).type === 'mention' ? true : isVoid(element)
  }

  editor.markableVoid = (element) => {
    return (element as CustomElement).type === 'mention' || markableVoid(element)
  }

  return editor
}

const insertMention = (editor: CustomEditor, user: ApiUser) => {
  const displayName = user.display_name || user.username
  const mention: MentionElement = {
    type: 'mention',
    character: displayName,
    children: [{ text: '' }],
  }
  Transforms.insertNodes(editor, mention)
  Transforms.move(editor)
}

const toggleBlock = (editor: CustomEditor, format: CustomElementFormat) => {
  const isActive = isBlockActive(
    editor,
    format,
    isAlignType(format) ? 'align' : 'type'
  )
  const isList = isListType(format)

  Transforms.unwrapNodes(editor, {
    match: n =>
      !Editor.isEditor(n) &&
      SlateElement.isElement(n) &&
      isListType(n.type) &&
      !isAlignType(format),
    split: true,
  })
  let newProperties: Partial<SlateElement>
  if (isAlignType(format)) {
    newProperties = {
      align: isActive ? undefined : format,
    }
  } else {
    newProperties = {
      type: isActive ? 'paragraph' : isList ? 'list-item' : format,
    }
  }
  Transforms.setNodes<SlateElement>(editor, newProperties)

  if (!isActive && isList) {
    const block = { type: format, children: [] }
    Transforms.wrapNodes(editor, block)
  }
}

const toggleMark = (editor: CustomEditor, format: CustomTextKey) => {
  const isActive = isMarkActive(editor, format)

  if (isActive) {
    Editor.removeMark(editor, format)
  } else {
    Editor.addMark(editor, format, true)
  }
}

const isBlockActive = (
  editor: CustomEditor,
  format: CustomElementFormat,
  blockType: 'type' | 'align' = 'type'
) => {
  const { selection } = editor
  if (!selection) return false

  const [match] = Array.from(
    Editor.nodes(editor, {
      at: Editor.unhangRange(editor, selection),
      match: n => {
        if (!Editor.isEditor(n) && SlateElement.isElement(n)) {
          if (blockType === 'align' && isAlignElement(n)) {
            return n.align === format
          }
          return n.type === format
        }
        return false
      },
    })
  )

  return !!match
}

const isMarkActive = (editor: CustomEditor, format: CustomTextKey) => {
  const marks = Editor.marks(editor)
  return marks ? marks[format] === true : false
}

const Element = ({ attributes, children, element }: RenderElementProps) => {
  const style: React.CSSProperties = {}
  if (isAlignElement(element)) {
    style.textAlign = element.align as AlignType
  }

  // Handle mention elements
  if (element.type === 'mention') {
    const mentionElement = element as MentionElement
    return (
      <span {...attributes} contentEditable={false} className="mention">
        @{mentionElement.character}
        {children}
      </span>
    )
  }

  switch (element.type) {
    case 'block-quote':
      return (
        <blockquote style={style} {...attributes}>
          {children}
        </blockquote>
      )
    case 'bulleted-list':
      return (
        <ul style={style} {...attributes}>
          {children}
        </ul>
      )
    case 'heading-one':
      return (
        <h1 style={style} {...attributes}>
          {children}
        </h1>
      )
    case 'heading-two':
      return (
        <h2 style={style} {...attributes}>
          {children}
        </h2>
      )
    case 'list-item':
      return (
        <li style={style} {...attributes}>
          {children}
        </li>
      )
    case 'numbered-list':
      return (
        <ol style={style} {...attributes}>
          {children}
        </ol>
      )
    default:
      return (
        <p style={style} {...attributes}>
          {children}
        </p>
      )
  }
}

const Leaf = ({ attributes, children, leaf }: RenderLeafProps) => {
  if (leaf.bold) {
    children = <strong>{children}</strong>
  }

  if (leaf.code) {
    children = <code>{children}</code>
  }

  if (leaf.italic) {
    children = <em>{children}</em>
  }

  if (leaf.underline) {
    children = <u>{children}</u>
  }

  return <span {...attributes}>{children}</span>
}

interface BlockButtonProps {
  format: CustomElementFormat
  icon: string
}

const BlockButton = ({ format, icon }: BlockButtonProps) => {
  const editor = useSlate()
  return (
    <Button
      active={isBlockActive(
        editor,
        format,
        isAlignType(format) ? 'align' : 'type'
      )}
      onPointerDown={(event: PointerEvent<HTMLButtonElement>) =>
        event.preventDefault()
      }
      onClick={() => toggleBlock(editor, format)}
      data-test-id={`block-button-${format}`}
    >
      <Icon>{icon}</Icon>
    </Button>
  )
}

interface MarkButtonProps {
  format: CustomTextKey
  icon: string
}

const MarkButton = ({ format, icon }: MarkButtonProps) => {
  const editor = useSlate()
  return (
    <Button
      active={isMarkActive(editor, format)}
      onPointerDown={(event: PointerEvent<HTMLButtonElement>) =>
        event.preventDefault()
      }
      onClick={() => toggleMark(editor, format)}
    >
      <Icon>{icon}</Icon>
    </Button>
  )
}

const isAlignType = (format: CustomElementFormat): format is AlignType => {
  return TEXT_ALIGN_TYPES.includes(format as AlignType)
}

const isListType = (format: CustomElementFormat): format is ListType => {
  return LIST_TYPES.includes(format as ListType)
}

const isAlignElement = (
  element: CustomElement
): element is CustomElementWithAlign => {
  return 'align' in element
}

const defaultInitialValue: Descendant[] = [
  {
    type: 'paragraph',
    children: [
      { text: 'This is a ' },
      { text: 'rich text editor', bold: true },
      { text: ' with ' },
      { text: 'markdown support', italic: true },
      { text: ' and ' },
      { text: '@mentions', code: true },
      { text: '!' },
    ],
  },
  {
    type: 'paragraph',
    children: [
      {
        text: "Type @ to mention someone. You can format text with toolbar buttons or keyboard shortcuts:",
      },
    ],
  },
  {
    type: 'bulleted-list',
    children: [
      {
        type: 'list-item',
        children: [{ text: 'Cmd/Ctrl+B for ', bold: false }, { text: 'bold', bold: true }],
      },
      {
        type: 'list-item',
        children: [{ text: 'Cmd/Ctrl+I for ', bold: false }, { text: 'italic', italic: true }],
      },
      {
        type: 'list-item',
        children: [{ text: 'Cmd/Ctrl+U for ', bold: false }, { text: 'underline', underline: true }],
      },
      {
        type: 'list-item',
        children: [{ text: 'Cmd/Ctrl+` for ', bold: false }, { text: 'code', code: true }],
      },
    ],
  },
  {
    type: 'paragraph',
    children: [{ text: 'Type @ followed by a username to mention someone!' }],
  },
]

export default RichTextExample
