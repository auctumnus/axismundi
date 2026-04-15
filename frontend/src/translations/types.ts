import type { BaseEditor, Descendant } from 'slate'
import type { ReactEditor } from 'slate-react'

export interface QuotationWithWordInfo {
    id: string;
    span_start: number;
    span_end: number;
    definition_id: string;
    definition_text: string;
    word_slug: string;
    word_lemma: number;
    word: string;
}

type CustomElement = { type: 'paragraph'; children: CustomText[] }
type CustomText = { text: string }

declare module 'slate' {
  interface CustomTypes {
    Editor: BaseEditor & ReactEditor
    Element: CustomElement
    Text: CustomText
  }
}