import type { BaseEditor, Descendant } from 'slate'
import type { HistoryEditor } from 'slate-history';
import type { ReactEditor } from 'slate-react'

export interface QuotationWithWordInfo {
  id: string;
  span_start: number;
  span_end: number;
  highlight_start: number | null;
  highlight_end: number | null;
  notes: string;
  definition_id: string;
  definition_text: string;
  word_slug: string;
  word_lemma: number;
  word: string;
}

export interface QuotationPossiblyNew extends Omit<QuotationWithWordInfo, 'id'> {
  id?: string;
}

export type TextElement = {
  type: 'text';
  text: string;
  quotation?: QuotationPossiblyNew;
}

export type ParagraphElement = {
  type: 'paragraph';
  children: TextElement[];
}
export type QuotationsEditorElement = ParagraphElement

declare module 'slate' {
  interface CustomTypes {
    Editor: BaseEditor & ReactEditor & HistoryEditor;
    Element: QuotationsEditorElement
    Text: TextElement
  }
}