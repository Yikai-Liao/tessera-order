// SPDX-License-Identifier: BSD-2-Clause

import rust from "@ast-grep/lang-rust";
import { parse, registerDynamicLanguage, type SgNode } from "@ast-grep/napi";

registerDynamicLanguage({ rust });

// Match the repository's test markers through AST patterns, not a cfg parser.
function testOnly(attribute: SgNode) {
  if (attribute.matches("#[test]") || attribute.matches("#[cfg(test)]")) return true;
  const all = attribute.find("#[cfg(all($$$ARGS))]");
  if (
    all
      ?.getMultipleMatches("ARGS")
      .some((node) => node.kind() === "identifier" && node.text() === "test")
  )
    return true;
  if (
    attribute.find({ rule: { kind: "identifier", regex: "^test$" } }) &&
    !attribute.matches("#[cfg(not(test))]")
  )
    throw new Error("Unsupported test configuration");
  return false;
}

/** Tree-sitter owns Rust syntax, comments and literals; this walk selects production nodes. */
export function production(source: string) {
  const root = parse("rust", source).root();
  const lines: string[] = [];
  const testModules: string[] = [];
  function visit(node: SgNode) {
    const kind = String(node.kind());
    if (kind === "ERROR") throw new Error("Invalid Rust source");
    if (kind.endsWith("_comment")) return;
    if (node.isLeaf() || kind.endsWith("_literal")) {
      const line = node.range().start.line;
      for (const [offset, part] of node.text().split("\n").entries())
        if (part.trim()) lines[line + offset] = `${lines[line + offset] ?? ""} ${part}`;
      return;
    }
    const children = node.children();
    if (
      children.some(
        (child) =>
          child.kind() === "inner_attribute_item" &&
          child.find({ rule: { kind: "identifier", regex: "^test$" } }),
      )
    )
      throw new Error("Unsupported inner test configuration");
    let attributes: SgNode[] = [];
    for (const child of children) {
      if (String(child.kind()).endsWith("_comment")) continue;
      if (child.kind() === "attribute_item") {
        attributes.push(child);
        continue;
      }
      if (attributes.some(testOnly)) {
        if (child.kind() === "mod_item") {
          if (
            child.ancestors().some((parent) => parent.kind() === "mod_item") ||
            attributes.some((attr) => attr.matches("#[path = $P]")) ||
            child.find("#[path = $P]")
          )
            throw new Error("Unsupported test module path");
          testModules.push(child.field("name")!.text());
        }
      } else {
        for (const attr of attributes) visit(attr);
        visit(child);
      }
      attributes = [];
    }
  }
  visit(root);
  return { lines: lines.filter(Boolean).map((line) => line.trim()), testModules };
}
