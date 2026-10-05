import { useRef, useState } from "react";

export const useTableJsonFiles = <T,>({
  name,
  fallbackName,
  parse,
  serialize,
  onImport,
}: {
  name: string;
  fallbackName: string;
  parse: (source: string) => T;
  serialize: () => string;
  onImport: (value: T) => void;
}) => {
  const inputRef = useRef<HTMLInputElement>(null);
  const [reading, setReading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const importFile = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.currentTarget.files?.[0];
    event.currentTarget.value = "";
    if (!file) return;
    setReading(true);
    setError(null);
    setMessage(null);
    try {
      const value = parse(await file.text());
      onImport(value);
      setMessage(
        `Imported ${file.name}. Continue through the form to save your changes.`,
      );
    } catch (error) {
      setError(
        error instanceof Error ? error.message : "Could not read this file.",
      );
    } finally {
      setReading(false);
    }
  };

  const exportFile = () => {
    const blob = new Blob([serialize()], {
      type: "application/json;charset=utf-8",
    });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    const filename = name.replace(/[<>:"/\\|?*\u0000-\u001f]/g, "-").trim();
    link.href = url;
    link.download = `${filename || fallbackName}.json`;
    document.body.append(link);
    link.click();
    link.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  };

  return { inputRef, reading, error, message, importFile, exportFile };
};

export type JsonFiles = ReturnType<typeof useTableJsonFiles>;

export const JsonFileFeedback = ({ files }: { files: JsonFiles }) => (
  <>
    <input
      ref={files.inputRef}
      type="file"
      accept=".json,application/json"
      aria-label="Import table body JSON file"
      hidden
      onChange={files.importFile}
    />
    <div className="table-json-feedback">
      {files.error && (
        <div className="error" role="alert">
          {files.error}
        </div>
      )}
      <p className="hint" role="status">
        {files.message}
      </p>
    </div>
  </>
);

export const JsonFileIcon = ({
  direction,
}: {
  direction: "import" | "export";
}) => (
  <svg
    xmlns="http://www.w3.org/2000/svg"
    width="1em"
    height="1em"
    viewBox="0 0 24 24"
    aria-hidden="true"
  >
    {/* Material Symbols upload-rounded / download-rounded - https://github.com/google/material-design-icons/blob/master/LICENSE */}
    <path
      fill="currentColor"
      d={
        direction === "import"
          ? "M6 20q-.825 0-1.412-.587T4 18v-2q0-.425.288-.712T5 15t.713.288T6 16v2h12v-2q0-.425.288-.712T19 15t.713.288T20 16v2q0 .825-.587 1.413T18 20zm5-12.15L9.125 9.725q-.3.3-.712.288T7.7 9.7q-.275-.3-.288-.7t.288-.7l3.6-3.6q.15-.15.325-.212T12 4.425t.375.063t.325.212l3.6 3.6q.3.3.288.7t-.288.7q-.3.3-.712.313t-.713-.288L13 7.85V15q0 .425-.288.713T12 16t-.712-.288T11 15z"
          : "M11.625 15.513q-.175-.063-.325-.213l-3.6-3.6q-.3-.3-.288-.7t.288-.7q.3-.3.713-.312t.712.287L11 12.15V5q0-.425.288-.712T12 4t.713.288T13 5v7.15l1.875-1.875q.3-.3.713-.288t.712.313q.275.3.288.7t-.288.7l-3.6 3.6q-.15.15-.325.213t-.375.062t-.375-.062M6 20q-.825 0-1.412-.587T4 18v-2q0-.425.288-.712T5 15t.713.288T6 16v2h12v-2q0-.425.288-.712T19 15t.713.288T20 16v2q0 .825-.587 1.413T18 20z"
      }
    />
  </svg>
);
