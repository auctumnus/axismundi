import { expect, test } from "bun:test";
import {
  applyJsonUpdate,
  fromTree,
  intoKey,
  intoNodeId,
  intoTree,
  type JsonNodeID,
  type JsonObject,
  type JsonTree,
  type JsonUpdate,
} from "./extra-editor-tree";

function child(tree: JsonTree, parent: JsonNodeID, index: number): JsonNodeID {
  const node = tree.nodes[parent];
  if (node?.kind !== "collection") throw new Error("Expected collection");
  const id = node.children[index];
  if (!id) throw new Error("Missing child");
  return id;
}

function property(tree: JsonTree, parent: JsonNodeID, name: string) {
  const node = tree.nodes[parent];
  if (node?.kind !== "collection") throw new Error("Expected collection");
  for (const id of node.children) {
    const key = tree.nodes[id];
    if (key?.kind === "key" && key.key === name) return key;
  }
  throw new Error(`Missing property ${name}`);
}

function apply(tree: JsonTree, update: JsonUpdate): JsonTree {
  const original = structuredClone(tree);
  const result = applyJsonUpdate(tree, update);
  expect(tree).toEqual(original);
  if (!result.ok) throw new Error(result.error);
  // Every successful edit must still be JSON and have no unreachable nodes.
  const value = fromTree(result.tree);
  expect(JSON.parse(JSON.stringify(value))).toEqual(value);
  return result.tree;
}

function reject(tree: JsonTree, update: JsonUpdate, message: string): void {
  const original = structuredClone(tree);
  const result = applyJsonUpdate(tree, update);
  expect(result.ok).toBe(false);
  if (result.ok) throw new Error("Expected rejection");
  expect(result.error).toContain(message);
  expect(tree).toEqual(original);
}

test("round trips JSON, including special object keys", () => {
  const value = JSON.parse(
    '{"__proto__":{"safe":true},"constructor":null,"list":["",0,false,{}]}',
  );
  const tree = intoTree(value);
  const restored = fromTree(tree);
  expect(restored).toEqual(value);
  expect(Object.hasOwn(restored as object, "__proto__")).toBe(true);
  expect(Object.getPrototypeOf(restored)).toBe(Object.prototype);
});

test("replaces subtrees and the root while preserving the target ID", () => {
  const tree = intoTree({ nested: { list: [1, 2] }, keep: true });
  const nested = property(tree, tree.root, "nested");
  const next = apply(tree, {
    kind: "replace",
    node: nested.value,
    value: "text",
  });
  expect(fromTree(next)).toEqual({ nested: "text", keep: true });
  expect(property(next, next.root, "nested").value).toBe(nested.value);
  expect(Object.keys(next.nodes)).toHaveLength(5);
  const replaced = apply(next, {
    kind: "replace",
    node: next.root,
    value: [null],
  });
  expect(replaced.root).toBe(tree.root);
  expect(fromTree(replaced)).toEqual([null]);
  expect(Object.keys(replaced.nodes)).toHaveLength(2);
});

test("inserts and removes array entries and object properties", () => {
  const tree = intoTree({ list: [1, 3] });
  const list = property(tree, tree.root, "list").value;
  const inserted = apply(tree, {
    kind: "insert",
    value: 2,
    destination: { type: "array", parent: list, index: 1 },
  });
  const added = apply(inserted, {
    kind: "insert",
    value: { nested: [false] },
    destination: {
      type: "object",
      parent: tree.root,
      key: intoKey("new"),
      index: 0,
    },
  });
  expect(fromTree(added)).toEqual({
    new: { nested: [false] },
    list: [1, 2, 3],
  });
  expect(property(added, added.root, "new").id).toBe(
    child(added, added.root, 0),
  );
  const removed = apply(added, {
    kind: "remove",
    node: property(added, added.root, "new").value,
  });
  const final = apply(removed, {
    kind: "remove",
    node: child(removed, list, 1),
  });
  expect(fromTree(final)).toEqual({ list: [1, 3] });
  expect(Object.keys(final.nodes)).toHaveLength(5);
});

test("renames keys, allowing unchanged and special names", () => {
  const tree = intoTree({ first: 1, second: 2 });
  const key = property(tree, tree.root, "first");
  const unchanged = apply(tree, {
    kind: "rename-key",
    node: key.id,
    key: key.key,
  });
  const renamed = apply(unchanged, {
    kind: "rename-key",
    node: key.id,
    key: intoKey("__proto__"),
  });
  expect(property(renamed, renamed.root, "__proto__").value).toBe(key.value);
  expect(Object.keys(fromTree(renamed) as object)).toEqual([
    "__proto__",
    "second",
  ]);
  reject(
    tree,
    { kind: "rename-key", node: key.id, key: intoKey("second") },
    "Duplicate",
  );
});

test("reorders arrays using the index after detaching", () => {
  const tree = intoTree([1, 2, 3]);
  const id = child(tree, tree.root, 0);
  const next = apply(tree, {
    kind: "move",
    node: id,
    destination: { type: "array", parent: tree.root, index: 2 },
  });
  expect(fromTree(next)).toEqual([2, 3, 1]);
  expect(child(next, next.root, 2)).toBe(id);
  reject(
    tree,
    {
      kind: "move",
      node: id,
      destination: { type: "array", parent: tree.root, index: 3 },
    },
    "out of bounds",
  );
});

test("reorders objects, including moves retaining the same key", () => {
  const tree = intoTree({ a: 1, b: 2 });
  const a = property(tree, tree.root, "a");
  const next = apply(tree, {
    kind: "move",
    node: a.value,
    destination: { type: "object", parent: tree.root, key: a.key, index: 1 },
  });
  expect(Object.keys(fromTree(next) as object)).toEqual(["b", "a"]);
  expect(property(next, next.root, "a").value).toBe(a.value);
});

test("moves values between objects and arrays without leaving key nodes behind", () => {
  const tree = intoTree({ item: { nested: true }, list: [] });
  const item = property(tree, tree.root, "item");
  const list = property(tree, tree.root, "list").value;
  const moved = apply(tree, {
    kind: "move",
    node: item.value,
    destination: { type: "array", parent: list, index: 0 },
  });
  expect(fromTree(moved)).toEqual({ list: [{ nested: true }] });
  expect(moved.nodes[item.id]).toBeUndefined();
  const restored = apply(moved, {
    kind: "move",
    node: item.value,
    destination: {
      type: "object",
      parent: tree.root,
      key: intoKey("restored"),
      index: 1,
    },
  });
  expect(fromTree(restored)).toEqual({ list: [], restored: { nested: true } });
  expect(property(restored, restored.root, "restored").value).toBe(item.value);
});

test("rejects duplicate keys without applying a partial move", () => {
  const tree = intoTree({ a: 1, b: 2 });
  reject(
    tree,
    {
      kind: "move",
      node: property(tree, tree.root, "a").value,
      destination: {
        type: "object",
        parent: tree.root,
        key: intoKey("b"),
        index: 0,
      },
    },
    "Duplicate",
  );
  reject(
    tree,
    {
      kind: "insert",
      value: { nested: true },
      destination: {
        type: "object",
        parent: tree.root,
        key: intoKey("a"),
        index: 0,
      },
    },
    "Duplicate",
  );
});

test("rejects root removal, root moves, and moves into descendants", () => {
  const tree = intoTree({ outer: { inner: [] } });
  const outer = property(tree, tree.root, "outer").value;
  const inner = property(tree, outer, "inner").value;
  reject(tree, { kind: "remove", node: tree.root }, "root");
  reject(
    tree,
    {
      kind: "move",
      node: tree.root,
      destination: { type: "array", parent: inner, index: 0 },
    },
    "own subtree",
  );
  reject(
    tree,
    {
      kind: "move",
      node: outer,
      destination: { type: "array", parent: inner, index: 0 },
    },
    "own subtree",
  );
});

test("rejects missing IDs and key nodes used as values", () => {
  const tree = intoTree({ key: 1 });
  reject(
    tree,
    { kind: "replace", node: intoNodeId("missing"), value: 2 },
    "not found",
  );
  const key = property(tree, tree.root, "key");
  for (const id of ["__proto__", "constructor", "toString"]) {
    reject(
      tree,
      { kind: "replace", node: intoNodeId(id), value: 2 },
      "not found",
    );
  }
  reject(tree, { kind: "replace", node: key.id, value: 2 }, "value node");
  reject(tree, { kind: "remove", node: key.id }, "value node");
  reject(
    tree,
    { kind: "rename-key", node: key.value, key: intoKey("new") },
    "key node",
  );
});

test("rejects mismatched destinations, noncollections, and invalid indices", () => {
  const tree = intoTree([0]);
  reject(
    tree,
    {
      kind: "insert",
      value: 1,
      destination: {
        type: "object",
        parent: tree.root,
        key: intoKey("key"),
        index: 0,
      },
    },
    "collection type",
  );
  reject(
    tree,
    {
      kind: "insert",
      value: 1,
      destination: {
        type: "array",
        parent: child(tree, tree.root, 0),
        index: 0,
      },
    },
    "collection type",
  );
  for (const index of [-1, 0.5, 2, NaN, Infinity]) {
    reject(
      tree,
      {
        kind: "insert",
        value: 1,
        destination: { type: "array", parent: tree.root, index },
      },
      "out of bounds",
    );
  }
});

test("rejects nonfinite numbers and cyclic values without discarding the old subtree", () => {
  const tree = intoTree({ keep: [1] });
  for (const value of [NaN, Infinity, -Infinity]) {
    expect(() => intoTree(value)).toThrow("finite");
    reject(
      tree,
      { kind: "replace", node: tree.root, value: { invalid: value } },
      "finite",
    );
    reject(
      tree,
      {
        kind: "insert",
        value,
        destination: {
          type: "object",
          parent: tree.root,
          key: intoKey("bad"),
          index: 0,
        },
      },
      "finite",
    );
  }
  const cyclic: JsonObject = {};
  cyclic.self = cyclic;
  expect(() => intoTree(cyclic)).toThrow("cycles");
  // Shared input objects are allowed; each occurrence gets its own tree nodes.
  const shared = { value: true };
  expect(fromTree(intoTree([shared, shared]))).toEqual([shared, shared]);
});

test("rejects malformed trees instead of accepting cycles, dangling or shared children", () => {
  const tree = intoTree([1]);
  const root = tree.nodes[tree.root];
  if (root?.kind !== "collection") throw new Error("Expected collection");
  const value = child(tree, tree.root, 0);
  for (const children of [
    [tree.root],
    [value, value],
    [intoNodeId("missing")],
    [],
  ]) {
    const malformed = {
      ...tree,
      nodes: { ...tree.nodes, [tree.root]: { ...root, children } },
    };
    expect(() => fromTree(malformed)).toThrow();
    expect(
      applyJsonUpdate(malformed, {
        kind: "replace",
        node: tree.root,
        value: null,
      }).ok,
    ).toBe(false);
  }
});
