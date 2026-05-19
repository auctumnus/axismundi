import {parser} from "./lexurgy-parser.js";
import {HighlightStyle, LRLanguage, LanguageSupport, indentService, syntaxHighlighting, syntaxTree} from "@codemirror/language";
import {styleTags, tags as t} from "@lezer/highlight";
import {autocompletion, closeBrackets, closeBracketsKeymap, CompletionContext} from "@codemirror/autocomplete";
import {type Diagnostic, linter, lintGutter} from "@codemirror/lint";
import type { SyntaxNode } from "@lezer/common";
import type { Text } from "@codemirror/state";
import { keymap } from "@codemirror/view";

console.log("[lexurgy lint] module loaded");

const lexurgyHighlighting = styleTags({
  Comment: t.lineComment,
  ElementKw: t.keyword,
  ClassKw: t.keyword,
  FeatureKw: t.keyword,
  FeatureModifier: t.keyword,
  DiacriticKw: t.keyword,
  DiacriticModifier: t.keyword,
  SymbolKw: t.keyword,
  KeywordPattern: t.keyword,
  SyllableKw: t.keyword,
  DeromanizerKw: t.keyword,
  RomanizerKw: t.keyword,
  LiteralKw: t.keyword,
  BlockTypeKw: t.keyword,
  KeywordModifier: t.keyword,
  KeywordExpression: t.keyword,
  Empty: t.keyword,
  SylBoundary: t.keyword,
  Boundary: t.keyword,
  BetweenWords: t.keyword,
  AnySyllable: t.keyword,
  "PlusFeatureDecl!": t.definition(t.propertyName),
  "FullFeature/Name": t.typeName,
  "FeatureVariable!": t.typeName,
  "NullAlias!": t.definition(t.propertyName),
  "FullFeature/FeatureValue!": t.definition(t.propertyName),
  "PlusFeatureValue!": t.propertyName,
  "FeatureValue!": t.propertyName,
  "AbsentFeature!": t.propertyName,
  "ElementDecl/Name": t.definition(t.variableName),
  "ClassDecl/Name": t.definition(t.variableName),
  "ElementRef!": t.variableName,
  "CaptureRef!": t.local(t.variableName),
  //"RuleName!": t.className,
  BlockRef: t.className,
  Anchor: t.operator,
  InterfixType: t.operator,
  "RepeaterType!": t.operator,
  '=> "/" "//" "!" :: ?:': t.operator,
  "( ) [ ] { } , :": t.punctuation,
});

export const lexurgyLanguage = LRLanguage.define({
  name: "lexurgy",
  parser: parser.configure({
    props: [lexurgyHighlighting],
  }),
  languageData: {
    commentTokens: {line: "#"},
  },
});

const lexurgyIndent = indentService.of((context, pos) => {
  // find the previous non-empty line
  let prevLine = context.state.doc.lineAt(pos);
  let prev = prevLine.number - 1;
  while (prev >= 1) {
    prevLine = context.state.doc.line(prev);
    if (prevLine.text.trim().length > 0) break;
    prev--;
  }

  const prevText = prevLine.text.trimEnd();

  // if previous line ends with ":", indent one level
  if (prevText.endsWith(":")) {
    return context.unit;
  }

  // if previous line is indented (inside a rule body), maintain it
  const prevIndent = prevLine.text.match(/^(\s*)/)?.[1]?.length ?? 0;
  if (prevIndent > 0) {
    return prevIndent;
  }

  return 0;
});

const pillTokens = (kind: string) => ({
  backgroundColor: `var(--editor-${kind}-bg)`,
  borderRadius: "var(--rounding)",
  boxShadow: `var(--editor-${kind}-shadow)`,
});

export const lexurgyHighlight = HighlightStyle.define([
  { tag: t.lineComment, color: "var(--editor-comment)" },
  { tag: t.keyword, color: "var(--editor-keyword)" },
  { tag: t.punctuation, color: "var(--editor-punctuation)" },
  { tag: t.variableName, color: "var(--editor-variable)", },
  { tag: t.propertyName, color: "var(--editor-property)", },
  { tag: t.definition(t.propertyName), color: "var(--editor-feature)", ...pillTokens("feature-definition") },
  { tag: t.propertyName, color: "var(--editor-feature)", ...pillTokens("feature") },
  { tag: t.definition(t.variableName), color: "var(--editor-class)", ...pillTokens("class-definition") },
  { tag: t.className, color: "var(--editor-class)", ...pillTokens("class") },
  { tag: t.variableName, color: "var(--editor-class)", ...pillTokens("class") },
  { tag: t.typeName, color: "var(--editor-feature)", ...pillTokens("feature") },
]);

const findTop = (node: SyntaxNode): SyntaxNode => {
  let current = node;
  while (current.parent) {
    current = current.parent;
  }
  return current;
}

type FeatureKind = "binary" | "multivalent" | "univalent";

const findFeatures = (doc: Text, top: SyntaxNode, until: number) => {
  const features: {name: string, kind: FeatureKind}[] = [];
  top.getChildren("FeatureDecl").forEach(featureNode => {
    if (featureNode.to <= until) {
      const fullFeatures =
        featureNode.getChildren("FullFeature")
          .map(n => n.getChildren("FeatureValue"))
          .flat()
          .map(n => doc.sliceString(n.from, n.to))
          .map(name => ({name, kind: "multivalent" as FeatureKind}));

      const plusFeatures =
        featureNode.getChildren("PlusFeature")
        .map(n => doc.sliceString(n.from, n.to))
        .map(name => ({name, kind: "binary" as FeatureKind}));
      
      features.push(...fullFeatures, ...plusFeatures);
    }
  })
  return features;
}

const findClasses = (doc: Text, top: SyntaxNode, until: number) => {
  const classes: string[] = [];
  top.getChildren("ClassDecl").forEach(classNode => {
    if (classNode.to <= until) {
      const nameNode = classNode.getChild("Name");
      if (nameNode) {
        const name = doc.sliceString(nameNode.from, nameNode.to);
        classes.push(name);
      }
    }
  })
  return classes;
}


const keywordCompletions = [
  "Feature",
  "Symbol",
  "Diacritic",
  "Class",
  "romanizer",
  "deromanizer",
  "romanizer literal",
  "deromanizer literal",
].map(kw => ({label: kw, type: "keyword"}));

const autocomplete = (context: CompletionContext) => {
  const nodeBefore = syntaxTree(context.state).resolveInner(context.pos, -1);
  const top = findTop(nodeBefore);

  // setTimeout(() => { debugger; }, 1000)

  switch (nodeBefore.type.name) {
    case "Name": {
      // need to find parent to know what we can complete
      const parent = nodeBefore.parent;
      if (!parent) break;

      if (parent.type.name === "ElementRef") {
        // complete the rest of the class name
        const classes = findClasses(context.state.doc, top, context.pos);
        const alreadyTyped = context.state.sliceDoc(nodeBefore.from, context.pos);
        return {
          from: nodeBefore.from,
          options: classes
            .filter(c => c.startsWith(alreadyTyped))
            .map(c => ({label: c, type: "class"})),
        }
      }

      if (parent.type.name === "FeatureValue" || parent.type.name === "PlusFeatureValue") {
        // complete the rest of the feature name
        const features = findFeatures(context.state.doc, top, context.pos);
        const alreadyTyped = context.state.sliceDoc(nodeBefore.from, context.pos);
        console.log(parent.type.name)
        const desiredFeatureKind = parent.type.name === "FeatureValue" ? "multivalent" : "binary";
        return {
          from: nodeBefore.from,
          options: features
            .filter(f => f.name.startsWith(alreadyTyped) && f.kind === desiredFeatureKind)
            .map(f => ({label: f.name, type: "feature"})),
        }
      }

      if(parent.type.name === "RuleName" && parent.parent?.parent?.type.name === "file") {
        // if at the top level, this could also be a declaration; suggest declaration keywords
        return {
          from: nodeBefore.from,
          options: keywordCompletions,
        };
      }

      if(parent?.parent?.type.name === "RuleElement") {
        // could Then, Else, `unchanged`
        return {
          from: nodeBefore.from,
          options: [
            {label: "Then", type: "keyword"},
            {label: "Else", type: "keyword"},
            {label: "unchanged", type: "keyword"},
          ],
        };
      }

      break;
    }
    case "ElementRef": {
      // just started typing @, suggest all classes
      const classes = findClasses(context.state.doc, top, context.pos);
      return {
        from: context.pos,
        options: classes.map(c => ({label: c, type: "class"})),
      }
    }
    case "[":
    case "FancyMatrix":
    case "PlusFeatureValue":
    case "Matrix": {
      // inside a matrix, suggest all features
      const features = findFeatures(context.state.doc, top, context.pos);
      return {
        from: context.pos,
        options: features.map(f => ({label: f.name, type: "feature"})),
      }
    }
    case "ChangeRule": {
      if (!nodeBefore.getChild(":")) {
        // could be a rule modifier
        return {
          from: context.pos,
          options: [
            {label: "defer", type: "keyword"},
            {label: "propagate", type: "keyword"},
            {label: "cleanup", type: "keyword"},
            {label: "ltr", type: "keyword"},
          ],
        };
      }
    }
  }

  return null;
}

const completions = lexurgyLanguage.data.of({
  autocomplete,
})

const lexurgyLinter = linter(view => {
  const diagnostics: Diagnostic[] = [];
  const tree = syntaxTree(view.state);
  const doc = view.state.doc;

  const declared = new Set<string>();
  const refs: {from: number, to: number, name: string}[] = [];

  const cursor = tree.cursor();
  do {
    if (cursor.name === "ClassDecl" || cursor.name === "ElementDecl") {
      const nameNode = cursor.node.getChild("Name");
      if (nameNode) declared.add(doc.sliceString(nameNode.from, nameNode.to));
    } else if (cursor.name === "ElementRef") {
      const nameNode = cursor.node.getChild("Name");
      if (nameNode) {
        refs.push({
          from: cursor.from,
          to: cursor.to,
          name: doc.sliceString(nameNode.from, nameNode.to),
        });
      }
    }
  } while (cursor.next());

  console.log("[lexurgy lint] declared:", [...declared], "refs:", refs);

  for (const ref of refs) {
    if (!declared.has(ref.name)) {
      diagnostics.push({
        from: ref.from,
        to: ref.to,
        severity: "error",
        message: `undefined class: @${ref.name}`,
        markClass: "undefined-name",
      });
    }
  }

  console.log("[lexurgy lint] diagnostics:", diagnostics);
  return diagnostics;
});

export const lexurgy = () => {
  return new LanguageSupport(lexurgyLanguage, [lexurgyIndent, syntaxHighlighting(lexurgyHighlight), completions, autocompletion(), closeBrackets(), keymap.of(closeBracketsKeymap), lexurgyLinter]);
}
