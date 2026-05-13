// Code for the sound change runner page; by weight, this is mostly the editor.

import {EditorState} from "@codemirror/state"
import {drawSelection, EditorView, highlightActiveLine, highlightActiveLineGutter, keymap, lineNumbers, ViewUpdate, type KeyBinding} from "@codemirror/view"
import {defaultKeymap, history, historyKeymap, indentWithTab} from "@codemirror/commands"
import {indentUnit, syntaxHighlighting, defaultHighlightStyle, syntaxTree} from "@codemirror/language"
import {lexurgy} from "./lexurgy-language"
import { acceptCompletion, completionKeymap } from "@codemirror/autocomplete"
import text from "slate-react/dist/components/text"
import { mountCombobox, mountRunButton, mountSaveButton } from "./sound-changer-buttons"

export const theme = EditorView.theme({
    "&": {
        height: "100%",
        backgroundColor: "var(--editor-bg)",
        border: "1px solid var(--editor-border);",
        borderRadius: "var(--rounding)",
        overflow: "hidden",
    },
    ".cm-content": {
        fontFamily: "var(--editor-font)",
    },
    ".cm-scroller": { overflow: "auto" },
    ".cm-gutters": {
        border: "0px solid transparent",
        padding: "0",
        paddingLeft: "4px",
    },
    ".cm-gutters.cm-gutters-before": {
        borderRightWidth: "0px !important",
        paddingLeft: "0px !important",
    },
    ".cm-gutter": {
        fontFamily: "var(--editor-font)",
        backgroundColor: "var(--gutter-bg)",
        color: "var(--gutter-fg)",
        borderRight: "0px solid var(--gutter-border)",
        borderTopLeftRadius: "var(--rounding)",
        borderBottomLeftRadius: "var(--rounding)",
        userSelect: "none",
    },
    "& .cm-lineNumbers .cm-gutterElement": {
        paddingRight: ".5rem !important",
        paddingLeft: ".5rem !important",
    },
    ".cm-activeLine": {
        backgroundColor: "var(--active-line-bg)",
    },
    "&.cm-focused .cm-selectionBackground, ::selection": {
        background: "var(--selection-bg-focused) !important",
    },
    "& .cm-selectionBackground": {
        background: "var(--selection-bg) !important",
    },
    ".cm-activeLineGutter": {
        backgroundColor: "var(--active-line-bg)",
    },
    ".cm-cursor": {
        borderLeft: "1.5px solid var(--cursor-color)",
    },
    "& .cm-tooltip.cm-tooltip-autocomplete > ul": {
        fontFamily: "var(--editor-font)",
    },
    "& .cm-tooltip": {
        border: "1px solid var(--tooltip-border)",
        borderRadius: "var(--rounding)",
        backgroundColor: "var(--tooltip-bg)",
    },
    "& .cm-tooltip-autocomplete ul li": {
        marginBottom: "0",
        padding: ".25rem",
    },
    "& .cm-tooltip-autocomplete ul li[aria-selected]": {
        backgroundColor: "var(--tooltip-highlight-bg)",
        color: "var(--tooltip-highlight-fg)",
    },
    "& .cm-completionIcon": {
        paddingRight: "0px",
        marginRight: ".5rem",
        position: "relative",
        top: ".15rem",
    },
    "& .cm-completionIcon-class::after": {
        display: "none",
    },
    "& .cm-completionIcon-class": {
        width: "1rem",
        height: "1rem",
        backgroundImage: "var(--icon-class)",
        backgroundSize: "contain",
    },
    "& .cm-completionIcon-feature": {
        width: "1rem",
        height: "1rem",
        backgroundImage: "var(--icon-feature)",
        backgroundSize: "contain",
    },
    "& .cm-completionIcon-keyword::after": {
        display: "none",
    },
    "& .cm-completionIcon-keyword": {
        width: "1rem",
        height: "1rem",
        backgroundImage: "var(--icon-keyword)",
        backgroundSize: "contain",
    }

});

const completeWithTab: KeyBinding = {
    key: "Tab",
    run: acceptCompletion,
}

const changeListeners: ((changes: ViewUpdate) => any)[] = []

const attachEditor = (parent: Element, doc: string): EditorView => {
    const view = new EditorView({
        doc,
        parent,
        extensions: [
            indentUnit.of("  "),
            lineNumbers(),
            history(),
            drawSelection(),
            highlightActiveLine(),
            highlightActiveLineGutter(),
            keymap.of([
                ...defaultKeymap,
                ...historyKeymap,
                ...completionKeymap,
                completeWithTab,
                indentWithTab,
            ]),
            lexurgy(),
            syntaxHighlighting(defaultHighlightStyle),
            EditorView.updateListener.of((update) => {
                changeListeners.forEach((listener) => listener(update));
            }),
            theme
        ]
    })

    return view;
}

const convertRemToPixels = (rem: number): number =>    
    rem * parseFloat(getComputedStyle(document.documentElement).fontSize);

let getChanges: (() => string) | null = null;


const setupEditor = (): boolean => {
    const changesElement = document.getElementById("changes") as HTMLTextAreaElement | null;
    if (!changesElement) {
        console.error("Could not find changes textarea to attach editor to.");
        return false;
    }

    const form = changesElement.closest("form");
    if (!form) {
        console.error("Could not find form for changes textarea.");
        return false;
    }

    // hidden input carries the editor content on form submit
    const hidden = document.createElement("input");
    hidden.type = "hidden";
    hidden.name = "changes";

    const changesDiv = document.createElement("div");
    changesDiv.className = "changes-editor-container";

    changesElement.replaceWith(changesDiv);
    form.appendChild(hidden);

    const view = attachEditor(changesDiv, changesElement.value || "");

    // associate the label with the editor's contenteditable
    view.contentDOM.setAttribute("aria-labelledby", "changes-label");
    view.contentDOM.setAttribute("role", "textbox");
    view.contentDOM.setAttribute("aria-multiline", "true");

    (document.querySelector(".changes-editor-container")! as HTMLDivElement).style.height = `${convertRemToPixels(25)}px`;

    // sync editor content into the hidden input before any form submission
    form.addEventListener("submit", () => {
        hidden.value = view.state.doc.toString();
    });

    getChanges = () => view.state.doc.toString();

    return true;
}

const setupCopyButtons = (): boolean => {
    const outputButtonRow = document.getElementById("output-button-row");

    if (!outputButtonRow) {
        // if there's no output, we also don't need the buttons
        return true;
    }

    const makeButton = (text: string, className: string, getOutput: (table: HTMLTableElement) => string) => {
        const button = document.createElement("button");
        button.type = "button";
        button.textContent = text;
        button.className = `normal ${className}`;
        button.addEventListener("click", () => {
            const outputTable = document.getElementById("output") as HTMLTableElement | null;
            if (!outputTable) return;

            const text = getOutput(outputTable);

            navigator.clipboard.writeText(text);

            button.textContent = "Copied!";
            setTimeout(() => {
                button.textContent = text;
            }, 2000);
        });

        outputButtonRow.appendChild(button);
    }

    makeButton("Copy output", "copy-output", (outputTable) =>
        Array.from(outputTable.rows)
            .map(row => row.cells[row.cells.length - 1]!.textContent?.trim() || "")
            .join("\n")
    );

    makeButton("Copy input and output", "copy-input-output", (outputTable) =>
        Array.from(outputTable.rows)
            .map(row => Array.from(row.cells).map(cell => cell.textContent?.trim() || "").join("\t"))
            .join("\n")
    );

    return true;
}

export interface Request {                                                          
    changes: string;                                                                                 
    inputWords: string[];                                                                            
    traceWords?: string[];                                                                           
    startAt?: string;                                                                                
    stopBefore?: string;                                                                           
    allowPolling?: boolean;
}

export interface TraceStep {                                                                            
    rule: string;                                                                                    
    output: string;                                                                                  
}                                                                                                  
                                                                                                    
export interface RuleFailure {                                                                          
    message: string;
    rule: string;
    originalWord: string;
    currentWord: string;
}

export interface Response {
    ruleNames: string[];
    outputWords: string[];                                                                           
    intermediateWords?: Record<string, string[]>;
    traces?: Record<string, TraceStep[]>;                                                            
    errors?: RuleFailure[];                                                                        
}

export type Error =                                                                                     
    | { type: "parseError"; message: string; lineNumber: number; columnNumber: number }              
    | { type: "invalidExpression"; message: string; rule: string; expression: string;                
  expressionNumber: number }                                                                         
    | { type: "analysisError"; message: string }                                                     
    | { type: "runtimeError"; message: string }                                                      
    | { type: "timeout"; message: string };

const tableFromRows = (rows: HTMLTableRowElement[]): HTMLTableElement => {
    const table = document.createElement("table");
    table.id = "output";

    const header = document.createElement("thead");
    const headerRow = document.createElement("tr");
    const inputHeader = document.createElement("th");
    inputHeader.textContent = "Input word";
    const outputHeader = document.createElement("th");
    outputHeader.textContent = "Output word";
    headerRow.appendChild(inputHeader);
    headerRow.appendChild(outputHeader);
    header.appendChild(headerRow);
    table.appendChild(header);

    table.ariaLabelledByElements = [document.querySelector("#output-label") as HTMLElement];
    
    const body = document.createElement("tbody");
    rows.forEach(row => body.appendChild(row));
    table.appendChild(body);

    return table;
}

const setupSubmitButtons = (): boolean => {
    const runButton = document.getElementById("run-button") as HTMLButtonElement | null;
    const saveButton = document.getElementById("save-button") as HTMLButtonElement | null;

    if(saveButton) {
        const languageCode = saveButton.getAttribute("data-language-code");
        const setId = saveButton.getAttribute("data-set-id");

        console.log("Setting up submit buttons with language code", languageCode, "and set ID", setId);

        if (!languageCode || !setId) {
            console.error("Save button is missing data attributes for language code or set ID.");
            return false;
        }

        const changes = document.querySelector("input[name='changes']") as HTMLInputElement | null;
        if (!changes) {
            console.error("Could not find hidden input for changes.");
            return false;
        }

        if (!getChanges) {
            console.error("getChanges function is not defined.");
            return false;
        }

        const newSaveButtonContainer = document.createElement("div");
        newSaveButtonContainer.className = "save-button-container";
        saveButton.replaceWith(newSaveButtonContainer);

        mountSaveButton(newSaveButtonContainer, languageCode, setId, getChanges);
    }

    if (!runButton) {
        return false;
    }

    const inputWords = document.getElementById("input-words") as HTMLTextAreaElement | null;
    if (!inputWords) {
        console.error("Could not find input words element.");
        return false;
    }

    const errorContainer = document.getElementById("error-container");
    if (!errorContainer) {
        console.error("Could not find error container element.");
        return false;
    }

    let outputTable = document.querySelector("table#output") as HTMLTableElement | null;
    // it's fine if we don't have an output table yet; we start with a `div.empty-output` at first
    let emptyOutput = document.querySelector(".empty-output") as HTMLDivElement | null;

    const traces = document.getElementById("traces") as HTMLDivElement | null;
    if (!traces) {
        console.error("Could not find traces container element.");
        return false;
    }

    const mountTable = (table: HTMLTableElement) => {
        if (outputTable) {
            outputTable.replaceWith(table);
            outputTable = table;
        } else if(emptyOutput) {
            emptyOutput.replaceWith(table);
            outputTable = table;
            setupCopyButtons();
        } else {
            console.error("No existing output element to replace with the new table.");
        }
    }

    const getRequest = (): Request => {
        const input = inputWords.value || "";
        console.log(input)
        const words = input.split("\n").map(word => word.trim()).filter(word => word.length > 0);
        const changes = getChanges ? getChanges() : "";

        const startAtInput = document.getElementById("start_at") as HTMLInputElement | null;
        const stopBeforeInput = document.getElementById("stop_before") as HTMLInputElement | null;
        const traceWordsInput = document.getElementById("trace_words") as HTMLInputElement | null;

        const startAt = startAtInput?.value.trim() || undefined;
        const stopBefore = stopBeforeInput?.value.trim() || undefined;
        const traceWords = traceWordsInput?.value.trim() ? traceWordsInput.value.split("\n").map(word => word.trim()) : undefined;

        return {
            inputWords: words,
            changes,
            startAt,
            stopBefore,
            traceWords,
        }
    };

    const onResponse = (request: Request, response: Response) => {
        console.log("Received response from server:", response);
        const newRows = response.outputWords.map((outputWord, i) => {
            const row = document.createElement("tr");

            const inputCell = document.createElement("td");
            inputCell.textContent = request.inputWords[i] || "";

            const outputCell = document.createElement("td");
            outputCell.textContent = outputWord;

            row.appendChild(inputCell);
            row.appendChild(outputCell);
            return row;
        });

        const newTable = tableFromRows(newRows);
        mountTable(newTable);

        traces.innerHTML = "";

        if (response.traces) {
            for (const [input, steps] of Object.entries(response.traces)) {
                const table = document.createElement("table");

                const caption = document.createElement("caption");
                caption.textContent = `Trace for "${input}"`;
                table.appendChild(caption);

                const thead = document.createElement("thead");
                const headerRow = document.createElement("tr");
                for (const text of ["Applied Rule", "Input", "Output"]) {
                    const th = document.createElement("th");
                    th.textContent = text;
                    headerRow.appendChild(th);
                }
                thead.appendChild(headerRow);
                table.appendChild(thead);

                const tbody = document.createElement("tbody");

                for (let i = 0; i < steps.length; i++) {
                    const step = steps[i];
                    const stepInput = i === 0 ? input : steps[i - 1]!.output;
                    const tr = document.createElement("tr");
                    tr.innerHTML = `<td>${step!.rule}</td><td>${stepInput}</td><td>${step!.output}</td>`;
                    tbody.appendChild(tr);
                }

                table.appendChild(tbody);
                traces.appendChild(table);
            }
        }
    }

    const onError = (error: Error | string) => {
        console.error("Error running sound changes:", error);
        const errorMessage = typeof error === "string" ? error : error.message;
        errorContainer.innerHTML = "";
        const errorDiv = document.createElement("div");
        errorDiv.className = "error";
        const errorHeader = document.createElement("h2");
        errorHeader.textContent = "Error";
        const errorParagraph = document.createElement("p");
        errorParagraph.className = "error";
        errorParagraph.textContent = errorMessage;
        errorDiv.appendChild(errorHeader);
        errorDiv.appendChild(errorParagraph);
        errorContainer.appendChild(errorDiv);
    }

    const runButtonContainer = document.createElement("div");
    runButton.replaceWith(runButtonContainer);

    mountRunButton(runButtonContainer, getRequest, onResponse, onError);

    return true;
}

const setupAdvancedOptions = (): boolean => {
    const advancedOptions = document.querySelector(".advanced-options") as HTMLDetailsElement | null;
    if (!advancedOptions) {
        console.error("Could not find advanced options element.");
        return false;
    }

    const summaryElement = advancedOptions.querySelector("summary");
    if (!summaryElement) {
        console.error("Could not find summary element within advanced options.");
        return false;
    }

    const contentElement = summaryElement.nextElementSibling;
    if (!contentElement) {
        console.error("Could not find content element following summary in advanced options.");
        return false;
    }

    summaryElement.addEventListener("click", (event) => {
        // from https://linkedlist.ch/animate_details_element_60/

        // Chrome sometimes has a hiccup and gets stuck.
        if (contentElement.classList.contains('animation')) {
        // So we make sure to remove those classes manually,
        contentElement.classList.remove('animation', 'collapsing');
        // ... enforce a reflow so that collapsing may be animated again,
        void summaryElement.offsetWidth;
        // ... and fallback to the default behaviour this time.
        return;
        }

        const onAnimationEnd = (cb: () => void) => contentElement.addEventListener(
            "animationend", cb, {once: true}
        );

        // request an animation frame to force Safari 16 to actually perform the animation
        requestAnimationFrame(() => contentElement.classList.add('animation'));
        onAnimationEnd(() => contentElement.classList.remove('animation'));

        const isDetailsOpen = advancedOptions.getAttribute('open') !== null;
        if (isDetailsOpen) {
        // prevent default collapsing and delay it until the animation has completed
        event.preventDefault();
        contentElement.classList.add('collapsing');
        onAnimationEnd(() => {
            advancedOptions.removeAttribute('open');
            contentElement.classList.remove('collapsing');
        });
        }
    });

    const startAtInput = document.getElementById("start_at") as HTMLInputElement | null;
    const stopBeforeInput = document.getElementById("stop_before") as HTMLInputElement | null;
    const traceWordsInput = document.getElementById("trace_words") as HTMLInputElement | null;

    // wow this is the hackiest store ever
    let rules: string[] = [];

    // startAt and stopBefore

    const rulesUpdateListeners: ((rules: string[]) => any)[] = [];

    const updateRules = (update: ViewUpdate) => {
        const top = syntaxTree(update.state).topNode;
        const newRules: string[] = [];
        top.getChildren("ChangeRule").forEach(ruleNode => {
            const nameNode = ruleNode.getChild("RuleName");
            if (nameNode) {
                const name = update.state.sliceDoc(nameNode.from, nameNode.to);
                newRules.push(name);
            }
        });
        // top.getChildren("InterRomanizer").forEach(romanizerNode => {
        //     const nameNode = romanizerNode.getChild("RuleName");
        //     if (nameNode) {
        //         const name = update.state.sliceDoc(nameNode.from, nameNode.to);
        //         newRules.push(`romanizer-${name}`);
        //     }
        // });
        rules = newRules;
        rulesUpdateListeners.forEach(listener => listener(rules));
    }
    
    changeListeners.push(updateRules);

    const rulesStore = {
        subscribe: (callback: (rules: string[]) => any) => {
            rulesUpdateListeners.push(callback);

            return () => {
                const index = rulesUpdateListeners.indexOf(callback);
                if (index !== -1) {
                    rulesUpdateListeners.splice(index, 1);
                }
            }
        },
        getSnapshot: () => {
            return rules;
        }
    }

    const setupRulesInput = (input: HTMLInputElement, title: string, description?: string) => {
        const name = input.name;
        const container = input.parentElement;
        if (!container) {
            console.error("Could not find parent element for rules input.");
            return;
        }

        const comboboxContainer = document.createElement("div");
        comboboxContainer.className = "rules-combobox-container";

        container.replaceWith(comboboxContainer);

        mountCombobox(comboboxContainer, {
            rulesStore,
            multiple: false,
            title,
            description,
            name,
        });
    }

    if (startAtInput) {
        setupRulesInput(startAtInput, "Rule to start from", "leave blank to start from the beginning");
    }

    if (stopBeforeInput) {
        setupRulesInput(stopBeforeInput, "Rule to stop before", "leave blank to run until the end");
    }

    // traceWords

    let words: string[] = [];
    const wordsInput = document.getElementById("input-words") as HTMLTextAreaElement | null;
    if (wordsInput) {
        words = wordsInput.value.split("\n").map(word => word.trim()).filter(word => word.length > 0);
    } else {
        console.error("Could not find input words element for setting up trace words input.");
        return false;
    }

    const wordsUpdateListeners: ((words: string[]) => any)[] = [];

    const updateWords = () => {
        const newWords = wordsInput.value.split("\n").map(word => word.trim()).filter(word => word.length > 0);
        words = newWords;
        wordsUpdateListeners.forEach(listener => listener(words));
    }

    wordsInput.addEventListener("change", updateWords);

    const wordsStore = {
        subscribe: (callback: (words: string[]) => any) => {
            wordsUpdateListeners.push(callback);

            return () => {
                const index = wordsUpdateListeners.indexOf(callback);
                if (index !== -1) {
                    wordsUpdateListeners.splice(index, 1);
                }
            }
        },
        getSnapshot: () => {
            return words;
        }
    }

    if (traceWordsInput) {
        const name = traceWordsInput.name;
        const container = document.createElement("div");
        container.className = "rules-combobox-container";

        const traceWordsInputParent = traceWordsInput.parentElement;
        if (!traceWordsInputParent) {
            console.error("Could not find parent element for trace words input.");
            return false;
        }

        traceWordsInputParent.replaceWith(container);

        mountCombobox(container, {
            rulesStore: wordsStore,
            multiple: true,
            title: "Words to trace",
            description: "to trace the evolution of words, enter one word per line",
            name,
        });
    }

    return true;
}

window.addEventListener("DOMContentLoaded", () => {
    if (!setupEditor()) {
        console.error("Failed to set up editor.");
        return;
    }

    if (!setupCopyButtons()) {
        console.error("Failed to set up copy buttons.");
        return;
    }

    if (!setupSubmitButtons()) {
        console.error("Failed to set up submit buttons.");
        return;
    }

    if (!setupAdvancedOptions()) {
        console.error("Failed to set up advanced options.");
        return;
    }
})