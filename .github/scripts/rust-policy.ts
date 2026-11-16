// SPDX-License-Identifier: BSD-2-Clause

import rust from "@ast-grep/lang-rust";
import { parse, registerDynamicLanguage, type SgNode } from "@ast-grep/napi";

registerDynamicLanguage({ rust });

const sourceRoot = "rust/tessera-order/src/";
const adapters = new Set(["algorithms", "cached", "parallel", "partition"]);

// Changes to the engine facade require reviewing the operation's full contract.
// Movement modules and workspace storage remain inaccessible to adapters.
const engineExports = new Set([
  "key::RadixKey",
  "key::ByKey",
  "key::Direct",
  "key::Extract",
  "key::Unsigned",
  "key::bit_len",
  "trace::Event",
  "trace::partition",
  "distribution::MAX_PARTITION_BUCKETS",
  "distribution::distribute",
  "rank::Requirement",
  "rank::normalize_indices",
  "refine::parallel::sort",
  "refine::refine",
  "refine::sort",
  "workspace::Workspace",
]);
const safetyLints = new Set([
  "clippy::undocumented_unsafe_blocks",
  "clippy::missing_safety_doc",
  "unsafe_op_in_unsafe_fn",
]);

/** Expand grouped imports before aliases, so a renamed import keeps its origin. */
function usePaths(node: SgNode): string[] {
  if (node.kind() === "use_as_clause") return usePaths(node.field("path")!);
  if (node.kind() === "use_list")
    return node
      .namedChildren()
      .filter((child) => !String(child.kind()).endsWith("comment"))
      .flatMap(usePaths);
  if (node.kind() === "scoped_use_list") {
    const prefix = node.field("path") ? pathText(node.field("path")!) : "";
    return usePaths(node.field("list")!).map((path) => `${prefix}::${path}`);
  }
  return [pathText(node)];
}

function pathText(node: SgNode): string {
  if (String(node.kind()).endsWith("comment")) return "";
  if (node.isLeaf()) return node.text().replaceAll("r#", "");
  return node.children().map(pathText).join("");
}

function lintSuppressions(node: SgNode): { lints: string[]; reason: boolean }[] {
  const result: { lints: string[]; reason: boolean }[] = [];
  const children = node.children();
  for (const [index, child] of children.entries()) {
    if (
      ["allow", "expect"].includes(child.text()) &&
      children[index + 1]?.kind() === "token_tree"
    ) {
      const args = children[index + 1]
        .children()
        .filter((arg) => !String(arg.kind()).endsWith("comment"));
      // Token trees preserve strings and comments as nodes. Reasons cannot
      // manufacture lint names or nested allow attributes.
      const names: string[] = [];
      let name = "";
      for (const arg of args.slice(1, -1)) {
        if (arg.text() === ",") {
          names.push(name);
          name = "";
        } else name += pathText(arg);
      }
      names.push(name);
      result.push({
        lints: names.filter((name) => /^[\w:]+$/.test(name)),
        reason: args.some(
          (arg, i) =>
            arg.text() === "reason" &&
            args[i + 1]?.text() === "=" &&
            ["string_literal", "raw_string_literal"].includes(String(args[i + 2]?.kind())) &&
            Boolean(
              args[i + 2]
                .find({ rule: { kind: "string_content" } })
                ?.text()
                .trim(),
            ),
        ),
      });
    }
    if (child.kind() !== "string_literal" && !String(child.kind()).endsWith("comment"))
      result.push(...lintSuppressions(child));
  }
  return result;
}

/** Check the private facade and safety-lint scope without changing Rust code. */
export function rustPolicy(path: string, source: string): string[] {
  if (!path.startsWith(sourceRoot) || !path.endsWith(".rs")) return [];
  const diagnostics: string[] = [];
  const root = parse("rust", source).root();
  const engine = path.startsWith(`${sourceRoot}engine/`);
  const facade = path === `${sourceRoot}engine/mod.rs`;
  const module = path
    .slice(sourceRoot.length)
    .replace(/(?:\/mod)?\.rs$/, "")
    .split("/");
  const report = (node: SgNode, rule: string, message: string) => {
    const { line, column } = node.range().start;
    diagnostics.push(`${path}:${line + 1}:${column + 1} [${rule}] ${message}`);
  };
  function checkPath(node: SgNode, reference: string) {
    const parts = reference.replaceAll("r#", "").split("::");
    const scope = [
      ...module,
      ...node
        .ancestors()
        .filter((ancestor) => ancestor.kind() === "mod_item")
        .reverse()
        .map((ancestor) => ancestor.field("name")!.text()),
    ];
    if (parts[0] === "crate") scope.length = 0;
    else if (parts[0] !== "super" && parts[0] !== "self") return;
    while (["crate", "super", "self"].includes(parts[0])) {
      if (parts.shift() === "super") scope.pop();
    }
    const target = [...scope, ...parts];
    if (adapters.has(target[0]) || target.length === 0 || target[0] === "*")
      report(
        node,
        "engine-boundary",
        "Use engine operations; do not import the adapter layer or the whole crate.",
      );
  }
  function visit(node: SgNode) {
    const kind = String(node.kind());
    if (kind.endsWith("comment") || kind.endsWith("literal")) return;
    if (engine && kind === "use_declaration")
      for (const reference of usePaths(node.field("argument")!)) checkPath(node, reference);
    if (
      engine &&
      kind === "scoped_identifier" &&
      node.parent()?.kind() !== "scoped_identifier" &&
      !node.ancestors().some((parent) => parent.kind() === "use_declaration")
    )
      checkPath(node, pathText(node));

    const visibility = node.children().find((child) => child.kind() === "visibility_modifier");
    if (path === `${sourceRoot}engine/workspace.rs` && visibility) {
      const workspaceField =
        ["field_declaration", "ordered_field_declaration"].includes(kind) &&
        node
          .ancestors()
          .some(
            (ancestor) =>
              ancestor.kind() === "struct_item" && ancestor.field("name")?.text() === "Workspace",
          );
      const workspaceMethod =
        kind === "function_item" &&
        node
          .ancestors()
          .some(
            (ancestor) =>
              ancestor.kind() === "impl_item" &&
              /(?:^|::)Workspace(?:<|$)/.test(pathText(ancestor.field("type")!)),
          );
      const withinEngine =
        /^pub\((?:self|super|in(?:self|super|crate::engine(?:::workspace)?))\)$/.test(
          pathText(visibility),
        );
      if (
        !withinEngine &&
        (workspaceField || (workspaceMethod && node.field("name")?.text() !== "new"))
      )
        report(
          node,
          "engine-boundary",
          "Keep Workspace storage opaque; only its reviewed constructor is visible outside the engine.",
        );
    }
    if (
      path === `${sourceRoot}lib.rs` &&
      kind === "mod_item" &&
      node.field("name")?.text() === "engine" &&
      visibility
    )
      report(node, "engine-boundary", "Keep the engine module private to the library.");
    if (facade && node.parent()?.kind() === "source_file" && visibility) {
      if (kind === "use_declaration") {
        for (const reference of usePaths(node.field("argument")!))
          if (!engineExports.has(reference))
            report(
              node,
              "engine-boundary",
              `Unreviewed engine export ${reference}; expose a complete operation, not movement state.`,
            );
      } else if (
        (kind !== "mod_item" || node.field("name")?.text() !== "thresholds") &&
        visibility.text().replaceAll(/\s/g, "") !== "pub(self)"
      ) {
        report(
          node,
          "engine-boundary",
          "Keep engine modules private and expose reviewed operations through the facade.",
        );
      }
    }
    if (kind === "attribute_item" || kind === "inner_attribute_item") {
      const target = node
        .nextAll()
        .find(
          (next) =>
            !["attribute_item", "line_comment", "block_comment"].includes(String(next.kind())),
        );
      for (const { lints, reason } of lintSuppressions(node)) {
        if (lints.some((lint) => ["warnings", "clippy::all"].includes(lint)))
          report(
            node,
            "safety-lint-scope",
            "Do not suppress warnings or clippy::all; name the individual lint.",
          );
        if (
          lints.some((lint) => safetyLints.has(lint)) &&
          (kind === "inner_attribute_item" ||
            !target ||
            ![
              "function_item",
              "function_signature_item",
              "expression_statement",
              "let_declaration",
            ].includes(String(target.kind())) ||
            !reason)
        )
          report(
            node,
            "safety-lint-scope",
            "Safety-lint exceptions require a specific function or statement and a nonempty reason.",
          );
      }
      return;
    }
    for (const child of node.children()) visit(child);
  }
  visit(root);
  return diagnostics;
}
