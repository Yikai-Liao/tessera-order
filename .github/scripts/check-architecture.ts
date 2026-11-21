// SPDX-License-Identifier: BSD-2-Clause

import { execFileSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import rust from "@ast-grep/lang-rust";
import { parse, registerDynamicLanguage, type SgNode } from "@ast-grep/napi";

registerDynamicLanguage({ rust });

const sourceRoot = "rust/tessera-order/src";
const engineRoot = `${sourceRoot}/engine`;

const safetyLints = new Set([
  "clippy::missing_safety_doc",
  "clippy::undocumented_unsafe_blocks",
  "unsafe_op_in_unsafe_fn",
]);
// These groups suppress warnings or include a protected safety lint.
const broadLintGroups = new Set([
  "clippy::all",
  "clippy::restriction",
  "clippy::style",
  "rust_2024_compatibility",
  "warnings",
]);
const localLintScopes = new Set([
  "expression_statement",
  "function_item",
  "function_signature_item",
  "let_declaration",
]);

function isComment(node: SgNode): boolean {
  return node.kind() === "line_comment" || node.kind() === "block_comment";
}

function syntaxText(node: SgNode): string {
  if (isComment(node)) return "";
  if (node.isLeaf()) return node.text().replace(/^r#/, "");
  return node.children().map(syntaxText).join("");
}

// Follow the source of each import, including grouped and aliased imports.
function usePaths(node: SgNode): string[] {
  switch (node.kind()) {
    case "use_as_clause":
      return usePaths(node.field("path")!);
    case "use_list":
      return node
        .namedChildren()
        .filter((child) => !isComment(child))
        .flatMap(usePaths);
    case "scoped_use_list": {
      const prefix = node.field("path");
      return usePaths(node.field("list")!).map(
        (path) => `${prefix ? syntaxText(prefix) : ""}::${path}`,
      );
    }
    default:
      return [syntaxText(node)];
  }
}

function inlineModuleDepth(node: SgNode): number {
  return node.ancestors().filter((parent) => parent.kind() === "mod_item").length;
}

function usesAdapters(node: SgNode, path: string, moduleDepth: number): boolean {
  const parts = path.split("::");
  if (parts.at(-1) === "self" && parts.length > 1) parts.pop(); // grouped self import
  const anchor = parts[0];
  let rootIndex = 1;
  if (anchor !== "crate") {
    if (anchor !== "super" && anchor !== "self") return false;
    rootIndex = anchor === "self" ? 1 : 0;
    const firstParent = rootIndex;
    while (parts[rootIndex] === "super") rootIndex += 1;
    if (rootIndex - firstParent !== moduleDepth + inlineModuleDepth(node)) return false;
  }
  const fromRoot = parts[rootIndex];
  return fromRoot === undefined || fromRoot === "*" || fromRoot === "algorithms";
}

const privateVisibility = new Set(["", "pub(self)", "pub(inself)"]);
const facadePrivateVisibility = new Set([...privateVisibility, "pub(incrate::engine)"]);

function visibility(node: SgNode): string {
  const modifier = node.children().find((child) => child.kind() === "visibility_modifier");
  return modifier ? syntaxText(modifier) : "";
}

function attributeArguments(tree: SgNode): SgNode[][] {
  const args: SgNode[][] = [[]];
  for (const token of tree.children().slice(1, -1)) {
    if (token.kind() === ",") args.push([]);
    else if (!isComment(token)) args.at(-1)!.push(token);
  }
  return args;
}

// cfg_attr's first argument is its predicate. The rest are attributes,
// including further cfg_attr lists. Strings and unrelated attributes stay opaque.
function lintSuppressions(tokens: SgNode[]): SgNode[][][] {
  const [name, tree] = tokens;
  if (tree?.kind() !== "token_tree") return [];
  switch (syntaxText(name)) {
    case "allow":
    case "expect":
      return [attributeArguments(tree)];
    case "cfg_attr":
      return attributeArguments(tree).slice(1).flatMap(lintSuppressions);
    default:
      return [];
  }
}

function hasReason(args: SgNode[][]): boolean {
  return args.some(
    ([name, equals, value]) =>
      name &&
      syntaxText(name) === "reason" &&
      equals?.kind() === "=" &&
      (value?.kind() === "string_literal" || value?.kind() === "raw_string_literal") &&
      value
        .namedChildren()
        .some((child) => child.kind() === "string_content" && child.text().trim()),
  );
}

function hasLocalLintScope(attribute: SgNode): boolean {
  if (attribute.kind() === "inner_attribute_item") return false;
  const [target, following] = attribute
    .nextAll()
    .filter((node) => node.isNamed() && !isComment(node) && node.kind() !== "attribute_item");
  if (!target) return false;
  const kind = String(target.kind());
  if (localLintScopes.has(kind)) return true;

  // The parser represents a block tail as a bare expression sibling.
  // A final declaration is still an item, even without a semicolon.
  return (
    attribute.parent()?.kind() === "block" &&
    !following &&
    kind !== "ERROR" &&
    !/_(item|declaration|definition|statement)$/.test(kind)
  );
}

/** Check the library's source boundaries and the scope of safety-lint exceptions. */
export function checkRustSource(path: string, source: string): string[] {
  const root = parse("rust", source).root();
  const diagnostics = new Set<string>();
  const facade = path === `${engineRoot}/mod.rs` || path === `${engineRoot}.rs`;
  const engine = facade || path.startsWith(`${engineRoot}/`);
  const report = (node: SgNode, rule: string, message: string) => {
    const { line, column } = node.range().start;
    diagnostics.add(`${path}:${line + 1}:${column + 1} [${rule}] ${message}`);
  };

  if (path === `${sourceRoot}/lib.rs`) {
    for (const item of root.namedChildren()) {
      if (
        item.kind() === "mod_item" &&
        syntaxText(item.field("name")!) === "engine" &&
        !privateVisibility.has(visibility(item))
      ) {
        report(item, "engine-boundary", "Keep the engine module private to the library.");
      }
    }
  }

  if (facade) {
    for (const item of root.namedChildren()) {
      if (facadePrivateVisibility.has(visibility(item))) continue;
      if (item.kind() === "use_declaration") {
        for (const origin of usePaths(item.field("argument")!)) {
          if (origin.endsWith("*")) {
            report(item, "engine-boundary", `Name each engine export instead of ${origin}.`);
          }
        }
      } else if (item.kind() !== "mod_item" || syntaxText(item.field("name")!) !== "thresholds") {
        report(
          item,
          "engine-boundary",
          "Keep engine modules private and expose reviewed operations through the facade.",
        );
      }
    }
  }

  if (path === `${engineRoot}/workspace.rs`) {
    for (const item of root.namedChildren()) {
      if (
        item.kind() === "struct_item" &&
        syntaxText(item.field("name")!) === "Workspace" &&
        visibility(item) !== "pub(incrate::engine)"
      ) {
        report(item, "engine-boundary", "Scope the Workspace type to crate::engine.");
      }
    }
  }

  if (engine) {
    const moduleDepth = path
      .slice(sourceRoot.length + 1)
      .replace(/\/mod\.rs$/, "")
      .split("/").length;
    const paths = root.findAll({
      rule: {
        any: [
          { kind: "use_declaration" },
          { kind: "scoped_identifier" },
          { kind: "scoped_type_identifier" },
        ],
      },
    });
    for (const node of paths) {
      const isUse = node.kind() === "use_declaration";
      const parentKind = node.parent()?.kind();
      if (
        !isUse &&
        (parentKind === "scoped_identifier" ||
          parentKind === "scoped_type_identifier" ||
          node.ancestors().some((parent) => parent.kind() === "use_declaration"))
      )
        continue;
      const references = isUse ? usePaths(node.field("argument")!) : [syntaxText(node)];
      if (references.some((reference) => usesAdapters(node, reference, moduleDepth))) {
        report(
          node,
          "engine-boundary",
          "Do not depend on the algorithm adapters or import the whole crate.",
        );
      }
    }
  }

  const attributes = root.findAll({
    rule: { any: [{ kind: "attribute_item" }, { kind: "inner_attribute_item" }] },
  });
  for (const node of attributes) {
    const attribute = node.namedChildren().find((child) => child.kind() === "attribute")!;
    const suppressions = lintSuppressions(
      attribute.children().filter((child) => !isComment(child)),
    );
    for (const args of suppressions) {
      const names = args.map((arg) => arg.map(syntaxText).join(""));
      const group = names.find((name) => broadLintGroups.has(name));
      if (group) {
        report(
          node,
          "safety-lint-scope",
          `Name the individual lint instead of suppressing ${group}.`,
        );
      }
      if (
        names.some((name) => safetyLints.has(name)) &&
        (!hasLocalLintScope(node) || !hasReason(args))
      ) {
        report(
          node,
          "safety-lint-scope",
          "Safety-lint exceptions require a specific function, statement or block tail expression and a nonempty reason.",
        );
      }
    }
  }
  return [...diagnostics];
}

export async function checkRepository(): Promise<string[]> {
  const paths = execFileSync(
    "git",
    ["ls-files", "-z", "--cached", "--others", "--exclude-standard", "--", `${sourceRoot}/*.rs`],
    { encoding: "utf8" },
  )
    .split("\0")
    .filter(Boolean);
  const diagnostics = await Promise.all(
    [...new Set(paths)].map(async (path) => {
      try {
        return checkRustSource(path, await readFile(path, "utf8"));
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code === "ENOENT") return []; // deleted in the worktree
        throw error;
      }
    }),
  );
  return diagnostics.flat().sort();
}

if (import.meta.main) {
  const diagnostics = await checkRepository();
  if (diagnostics.length) {
    console.error(diagnostics.join("\n"));
    process.exitCode = 1;
  } else {
    console.log("Rust architecture policies passed.");
  }
}
