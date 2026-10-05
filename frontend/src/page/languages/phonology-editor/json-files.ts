import { useTableJsonFiles } from "../table-json-files";
import { parseTableBody, serializeTableBody } from "./json";
import { useEditor } from "./state";

export const useJsonFiles = () => {
  const [state, dispatch] = useEditor();
  return useTableJsonFiles({
    name: state.name,
    fallbackName: "phonology-table",
    parse: parseTableBody,
    serialize: () => serializeTableBody(state.body),
    onImport: (body) => dispatch({ type: "ImportBody", body }),
  });
};
