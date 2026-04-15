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

export type TextElement = { type: 'text'; text: string }
export type QuotationElement = {
    type: 'quotation';
    definitionId: string;
    wordSlug: string;
    wordLemma: number;
    word: string;
}
export type QuotationsEditorElement = TextElement | QuotationElement

declare module 'slate' {
  interface CustomTypes {
    Editor: BaseEditor & ReactEditor
    Element: QuotationsEditorElement
    Text: TextElement
  }
}