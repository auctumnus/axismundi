export type JsonValue =
  | string
  | number
  | boolean
  | null
  | JsonObject
  | JsonValue[];
export type JsonObject = { [key: string]: JsonValue };

export type JsonPrimitiveValues = {
  string: string;
  number: number;
  boolean: boolean;
  null: null;
};

export type JsonType = keyof JsonPrimitiveValues | "object" | "array";
export type JsonKey = string & { __jsonKey: true };
export type JsonNodeID = string & { __jsonNodeId: true };

export interface JsonCollectionNode {
  id: JsonNodeID;
  kind: "collection";
  type: "object" | "array";
  children: JsonNodeID[]; // Object children are keys; array children are values.
}

export type JsonPrimitiveNode = {
  [T in keyof JsonPrimitiveValues]: {
    id: JsonNodeID;
    kind: "primitive";
    type: T;
    value: JsonPrimitiveValues[T];
  };
}[keyof JsonPrimitiveValues];

export interface JsonKeyNode {
  id: JsonNodeID;
  kind: "key";
  key: JsonKey;
  value: JsonNodeID;
}

export type JsonValueNode = JsonCollectionNode | JsonPrimitiveNode;
export type JsonNode = JsonValueNode | JsonKeyNode;

export interface JsonTree {
  root: JsonNodeID;
  nodes: Record<JsonNodeID, JsonNode>;
}

export type JsonDestination =
  | { type: "array"; parent: JsonNodeID; index: number }
  | { type: "object"; parent: JsonNodeID; key: JsonKey; index: number };

/** All node targets are values, except rename-key, which targets a key. */
export type JsonUpdate =
  | { kind: "replace"; node: JsonNodeID; value: JsonValue }
  | { kind: "insert"; destination: JsonDestination; value: JsonValue }
  | { kind: "remove"; node: JsonNodeID }
  | { kind: "rename-key"; node: JsonNodeID; key: JsonKey }
  // For moves, index refers to the destination after detaching the source.
  | { kind: "move"; node: JsonNodeID; destination: JsonDestination };

export type JsonUpdateResult =
  | { ok: true; tree: JsonTree }
  | { ok: false; error: string };

export const intoKey = (key: string): JsonKey => key as JsonKey;
export const intoNodeId = (id: string): JsonNodeID => id as JsonNodeID;
const newNodeId = (): JsonNodeID => intoNodeId(crypto.randomUUID());

function intoNode(
  value: JsonValue,
  nodes: JsonTree["nodes"],
  id = newNodeId(),
  ancestors = new Set<JsonObject | JsonValue[]>(),
): JsonValueNode {
  let node: JsonValueNode;
  if (value !== null && typeof value === "object") {
    if (ancestors.has(value))
      throw new Error("JSON values cannot contain cycles");
    ancestors.add(value);
    const children: JsonNodeID[] = [];
    if (Array.isArray(value)) {
      for (const child of value) {
        children.push(intoNode(child, nodes, newNodeId(), ancestors).id);
      }
    } else {
      for (const [key, child] of Object.entries(value)) {
        const childId = intoNode(child, nodes, newNodeId(), ancestors).id;
        const keyId = newNodeId();
        nodes[keyId] = {
          id: keyId,
          kind: "key",
          key: intoKey(key),
          value: childId,
        };
        children.push(keyId);
      }
    }
    ancestors.delete(value);
    node = {
      id,
      kind: "collection",
      type: Array.isArray(value) ? "array" : "object",
      children,
    };
  } else if (typeof value === "string") {
    node = { id, kind: "primitive", type: "string", value };
  } else if (typeof value === "number") {
    if (!Number.isFinite(value)) throw new Error("JSON numbers must be finite");
    node = { id, kind: "primitive", type: "number", value };
  } else if (typeof value === "boolean") {
    node = { id, kind: "primitive", type: "boolean", value };
  } else if (value === null) {
    node = { id, kind: "primitive", type: "null", value };
  } else {
    throw new Error("Unsupported JSON value");
  }
  nodes[id] = node;
  return node;
}

export function intoTree(value: JsonValue): JsonTree {
  const nodes: JsonTree["nodes"] = {};
  return { root: intoNode(value, nodes).id, nodes };
}

function getNode(tree: JsonTree, id: JsonNodeID): JsonNode {
  if (!Object.hasOwn(tree.nodes, id)) {
    throw new Error(`Node with id ${id} not found`);
  }
  const node = tree.nodes[id];
  if (!node) throw new Error(`Node with id ${id} not found`);
  if (node.id !== id)
    throw new Error(`Node id does not match its entry: ${id}`);
  return node;
}

function getValueNode(tree: JsonTree, id: JsonNodeID): JsonValueNode {
  const node = getNode(tree, id);
  if (node.kind === "key") throw new Error("Expected a value node");
  return node;
}

function getKeyNode(tree: JsonTree, id: JsonNodeID): JsonKeyNode {
  const node = getNode(tree, id);
  if (node.kind !== "key") throw new Error("Expected a key node");
  return node;
}

export function fromTree(tree: JsonTree): JsonValue {
  const visited = new Set<JsonNodeID>();
  const visit = (id: JsonNodeID): JsonNode => {
    if (visited.has(id))
      throw new Error("JSON nodes must have exactly one parent");
    visited.add(id);
    return getNode(tree, id);
  };
  const buildValue = (id: JsonNodeID): JsonValue => {
    const node = visit(id);
    if (node.kind === "key") throw new Error("Expected a value node");
    if (node.kind === "primitive") {
      if (node.type === "number" && !Number.isFinite(node.value)) {
        throw new Error("JSON numbers must be finite");
      }
      return node.value;
    }
    if (node.type === "array") return node.children.map(buildValue);
    const keys = new Set<JsonKey>();
    return Object.fromEntries(
      node.children.map((id) => {
        const key = visit(id);
        if (key.kind !== "key") throw new Error("Expected a key node");
        if (keys.has(key.key))
          throw new Error(`Duplicate object key: ${key.key}`);
        keys.add(key.key);
        return [key.key, buildValue(key.value)];
      }),
    );
  };
  const value = buildValue(tree.root);
  if (visited.size !== Object.keys(tree.nodes).length) {
    throw new Error("JSON tree contains unreachable nodes");
  }
  return value;
}

function findParent(tree: JsonTree, id: JsonNodeID): JsonCollectionNode {
  for (const node of Object.values(tree.nodes)) {
    if (node.kind === "collection" && node.children.includes(id)) return node;
  }
  throw new Error(`Parent for node ${id} not found`);
}

function subtreeIds(tree: JsonTree, id: JsonNodeID): JsonNodeID[] {
  const node = getNode(tree, id);
  const children =
    node.kind === "collection"
      ? node.children
      : node.kind === "key"
        ? [node.value]
        : [];
  return [id, ...children.flatMap((child) => subtreeIds(tree, child))];
}

function detach(tree: JsonTree, id: JsonNodeID): void {
  if (id === tree.root) throw new Error("Cannot remove or move the root");
  getValueNode(tree, id);
  const key = Object.values(tree.nodes).find(
    (node) => node.kind === "key" && node.value === id,
  );
  const entryId = key?.id ?? id;
  const parent = findParent(tree, entryId);
  parent.children.splice(parent.children.indexOf(entryId), 1);
  if (key) delete tree.nodes[key.id];
}

function attach(
  tree: JsonTree,
  id: JsonNodeID,
  destination: JsonDestination,
): void {
  const parent = getNode(tree, destination.parent);
  if (parent.kind !== "collection" || parent.type !== destination.type) {
    throw new Error("Destination must match the parent collection type");
  }
  if (
    !Number.isInteger(destination.index) ||
    destination.index < 0 ||
    destination.index > parent.children.length
  ) {
    throw new Error("Destination index is out of bounds");
  }
  let entryId = id;
  if (destination.type === "object") {
    if (
      parent.children.some(
        (child) => getKeyNode(tree, child).key === destination.key,
      )
    ) {
      throw new Error(`Duplicate object key: ${destination.key}`);
    }
    entryId = newNodeId();
    tree.nodes[entryId] = {
      id: entryId,
      kind: "key",
      key: destination.key,
      value: id,
    };
  }
  parent.children.splice(destination.index, 0, entryId);
}

/** Failed updates leave the original tree untouched, including its child arrays. */
export function applyJsonUpdate(
  tree: JsonTree,
  update: JsonUpdate,
): JsonUpdateResult {
  try {
    fromTree(tree);
    const next: JsonTree = {
      root: tree.root,
      nodes: Object.fromEntries(
        Object.entries(tree.nodes).map(([id, node]) => [
          id,
          node.kind === "collection"
            ? { ...node, children: [...node.children] }
            : { ...node },
        ]),
      ),
    };
    switch (update.kind) {
      case "replace": {
        getValueNode(next, update.node);
        for (const id of subtreeIds(next, update.node)) delete next.nodes[id];
        intoNode(update.value, next.nodes, update.node);
        break;
      }
      case "insert": {
        const node = intoNode(update.value, next.nodes);
        attach(next, node.id, update.destination);
        break;
      }
      case "remove": {
        detach(next, update.node);
        for (const id of subtreeIds(next, update.node)) delete next.nodes[id];
        break;
      }
      case "rename-key": {
        const key = getKeyNode(next, update.node);
        const parent = findParent(next, key.id);
        if (
          parent.children.some(
            (id) => id !== key.id && getKeyNode(next, id).key === update.key,
          )
        ) {
          throw new Error(`Duplicate object key: ${update.key}`);
        }
        key.key = update.key;
        break;
      }
      case "move": {
        getValueNode(next, update.node);
        if (subtreeIds(next, update.node).includes(update.destination.parent)) {
          throw new Error("Cannot move a value into its own subtree");
        }
        detach(next, update.node);
        attach(next, update.node, update.destination);
        break;
      }
    }
    return { ok: true, tree: next };
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}
