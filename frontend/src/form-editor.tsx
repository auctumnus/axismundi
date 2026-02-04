import React from "react";
import ReactDOM from "react-dom/client";
import RichTextEditor from "./text-editor";
import type { Descendant, Text } from "slate";
import { Element, Node } from "slate";
import type {
  CustomEditor,
  CustomElement,
  CustomText,
  MentionElement,
} from "./custom-types";

// Serialize Slate content to Markdown
function serializeToMarkdown(nodes: Descendant[]): string {
  return nodes.map((node) => serializeNode(node)).join("\n");
}

function serializeNode(node: Descendant): string {
  if (Element.isElement(node)) {
    const element = node as CustomElement;
    const children = element.children
      .map((child) => serializeNode(child))
      .join("");

    switch (element.type) {
      case "heading-one":
        return `# ${children}`;
      case "heading-two":
        return `## ${children}`;
      case "block-quote":
        return `> ${children}`;
      case "bulleted-list":
        return element.children
          .map((child) => {
            if (Element.isElement(child) && child.type === "list-item") {
              return `- ${child.children.map((c) => serializeNode(c)).join("")}`;
            }
            return "";
          })
          .join("\n");
      case "numbered-list":
        return element.children
          .map((child, index) => {
            if (Element.isElement(child) && child.type === "list-item") {
              return `${index + 1}. ${child.children.map((c) => serializeNode(c)).join("")}`;
            }
            return "";
          })
          .join("\n");
      case "list-item":
        return children;
      case "mention":
        const mention = element as MentionElement;
        return `@${mention.character}`;
      case "paragraph":
      default:
        return children;
    }
  }

  // Text node with formatting
  const text = node as CustomText;
  let result = text.text;

  // Handle @mentions in plain text (convert @username to proper mention)
  // This preserves @mentions when they're typed as plain text
  if (result.includes("@")) {
    return result;
  }

  // Apply markdown formatting
  if (text.code) {
    result = `\`${result}\``;
  }
  if (text.bold) {
    result = `**${result}**`;
  }
  if (text.italic) {
    result = `*${result}*`;
  }
  if (text.underline) {
    result = `_${result}_`;
  }

  return result;
}

// Parse Markdown into Slate content
function parseFromMarkdown(markdown: string): Descendant[] {
  if (!markdown || markdown.trim() === "") {
    return [
      {
        type: "paragraph" as const,
        children: [{ text: "" }],
      },
    ];
  }

  const lines = markdown.split("\n");
  const nodes: Descendant[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];

    // Heading 1
    if (line.startsWith("# ")) {
      nodes.push({
        type: "heading-one",
        children: parseInlineMarkdown(line.slice(2)),
      });
      i++;
      continue;
    }

    // Heading 2
    if (line.startsWith("## ")) {
      nodes.push({
        type: "heading-two",
        children: parseInlineMarkdown(line.slice(3)),
      });
      i++;
      continue;
    }

    // Block quote
    if (line.startsWith("> ")) {
      nodes.push({
        type: "block-quote",
        children: parseInlineMarkdown(line.slice(2)),
      });
      i++;
      continue;
    }

    // Bulleted list
    if (line.match(/^[-*]\s/)) {
      const listItems: any[] = [];
      while (i < lines.length && lines[i].match(/^[-*]\s/)) {
        listItems.push({
          type: "list-item",
          children: parseInlineMarkdown(lines[i].slice(2)),
        });
        i++;
      }
      nodes.push({
        type: "bulleted-list",
        children: listItems,
      });
      continue;
    }

    // Numbered list
    if (line.match(/^\d+\.\s/)) {
      const listItems: any[] = [];
      while (i < lines.length && lines[i].match(/^\d+\.\s/)) {
        const content = lines[i].replace(/^\d+\.\s/, "");
        listItems.push({
          type: "list-item",
          children: parseInlineMarkdown(content),
        });
        i++;
      }
      nodes.push({
        type: "numbered-list",
        children: listItems,
      });
      continue;
    }

    // Empty line
    if (line.trim() === "") {
      i++;
      continue;
    }

    // Regular paragraph
    nodes.push({
      type: "paragraph",
      children: parseInlineMarkdown(line),
    });
    i++;
  }

  return nodes.length > 0
    ? nodes
    : [{ type: "paragraph", children: [{ text: "" }] }];
}

// Parse inline markdown (bold, italic, code, mentions)
function parseInlineMarkdown(text: string): CustomText[] {
  const children: CustomText[] = [];

  // Simple regex-based parsing
  // This handles: **bold**, *italic*, `code`, _underline_, @mentions
  const regex = /(\*\*[^*]+\*\*|\*[^*]+\*|`[^`]+`|_[^_]+_|@\w+)/g;
  let lastIndex = 0;
  let match;

  while ((match = regex.exec(text)) !== null) {
    // Add text before the match
    if (match.index > lastIndex) {
      children.push({ text: text.slice(lastIndex, match.index) });
    }

    const matched = match[0];

    // Bold
    if (matched.startsWith("**") && matched.endsWith("**")) {
      children.push({ text: matched.slice(2, -2), bold: true });
    }
    // Italic
    else if (
      matched.startsWith("*") &&
      matched.endsWith("*") &&
      !matched.startsWith("**")
    ) {
      children.push({ text: matched.slice(1, -1), italic: true });
    }
    // Code
    else if (matched.startsWith("`") && matched.endsWith("`")) {
      children.push({ text: matched.slice(1, -1), code: true });
    }
    // Underline
    else if (matched.startsWith("_") && matched.endsWith("_")) {
      children.push({ text: matched.slice(1, -1), underline: true });
    }
    // Mention - keep as plain text for now, will be converted to mention node if needed
    else if (matched.startsWith("@")) {
      children.push({ text: matched });
    }

    lastIndex = regex.lastIndex;
  }

  // Add remaining text
  if (lastIndex < text.length) {
    children.push({ text: text.slice(lastIndex) });
  }

  return children.length > 0 ? children : [{ text: text }];
}

interface FormEditorProps {
  initialValue?: string;
  inputName: string;
  formId: string;
}

function FormEditor({ initialValue = "", inputName, formId }: FormEditorProps) {
  const editorRef = React.useRef<CustomEditor>(null);

  // Handle form submission
  React.useEffect(() => {
    const form = document.getElementById(formId);
    if (!form) return;

    const handleSubmit = () => {
      const hiddenInput = document.getElementById(
        `${inputName}-hidden`,
      ) as HTMLInputElement;
      if (hiddenInput && editorRef.current) {
        const content = editorRef.current.children as Descendant[];
        hiddenInput.value = serializeToMarkdown(content);
      }
    };

    form.addEventListener("submit", handleSubmit);
    return () => form.removeEventListener("submit", handleSubmit);
  }, [formId, inputName]);

  const initialContent = React.useMemo(() => {
    try {
      return parseFromMarkdown(initialValue);
    } catch (e) {
      console.error("Failed to parse initial value:", e);
      return parseFromMarkdown("");
    }
  }, [initialValue]);

  return (
    <div>
      <RichTextEditor ref={editorRef} initialValue={initialContent} />
      <input
        type="hidden"
        id={`${inputName}-hidden`}
        name={inputName}
        defaultValue={initialValue}
      />
    </div>
  );
}

// Mount the editor
export function mountFormEditor(
  containerId: string,
  options: {
    initialValue?: string;
    inputName: string;
    formId: string;
  },
) {
  const container = document.getElementById(containerId);
  if (!container) {
    console.error(`Container #${containerId} not found`);
    return;
  }

  const root = ReactDOM.createRoot(container);
  root.render(
    <FormEditor
      initialValue={options.initialValue}
      inputName={options.inputName}
      formId={options.formId}
    />,
  );
}

// Make it available globally for HTML templates
if (typeof window !== "undefined") {
  (window as any).mountFormEditor = mountFormEditor;
}
