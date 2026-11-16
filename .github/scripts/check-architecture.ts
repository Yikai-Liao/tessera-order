// SPDX-License-Identifier: BSD-2-Clause

import { execFileSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import { posix, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import ts from "typescript6";
import { rustPolicy } from "./rust-policy.ts";

const plotPackages = new Set(["@observablehq/plot", "linkedom/worker", "string-pixel-width"]);
const reportModules = new Set([
  "bench/catalog.ts",
  "bench/config.ts",
  "bench/plan.ts",
  "bench/statistics.ts",
  "bench/summary.ts",
]);
const reportPackages = new Set([
  ...plotPackages,
  "zod",
  "smol-toml",
  "fs",
  "fs/promises",
  "path",
  "url",
  "util",
]);
const synchronousCommands = new Set(["execSync", "execFileSync", "spawnSync"]);

function testSource(path: string) {
  return path.endsWith(".test.ts") || path.endsWith("/fixture.ts") || path.includes("/fixtures/");
}

function localModule(from: string, specifier: string) {
  return posix.normalize(posix.join(posix.dirname(from), specifier));
}

interface Dependency {
  node: ts.Node;
  specifier?: string;
  // Only explicit named synchronous subprocess imports may bypass command().
  names?: string[];
  typeOnly?: boolean;
}

function dependencies(source: ts.SourceFile): Dependency[] {
  const result: Dependency[] = [];
  function visit(node: ts.Node) {
    if (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) {
      const specifier = node.moduleSpecifier;
      if (specifier && ts.isStringLiteralLike(specifier)) {
        const clause = ts.isImportDeclaration(node) ? node.importClause : node.exportClause;
        const bindings = clause && ts.isImportClause(clause) ? clause.namedBindings : clause;
        result.push({
          node,
          specifier: specifier.text,
          typeOnly: ts.isImportDeclaration(node) ? node.importClause?.isTypeOnly : node.isTypeOnly,
          names:
            bindings &&
            (ts.isNamedImports(bindings) || ts.isNamedExports(bindings)) &&
            !(clause && ts.isImportClause(clause) && clause.name)
              ? bindings.elements
                  .filter((item) => !item.isTypeOnly)
                  .map((item) => (item.propertyName ?? item.name).text)
              : undefined,
        });
      }
    } else if (
      ts.isCallExpression(node) &&
      (node.expression.kind === ts.SyntaxKind.ImportKeyword ||
        (ts.isIdentifier(node.expression) && node.expression.text === "require"))
    ) {
      const argument = node.arguments[0];
      result.push({
        node,
        specifier: argument && ts.isStringLiteralLike(argument) ? argument.text : undefined,
      });
    } else if (ts.isExternalModuleReference(node)) {
      result.push({
        node,
        specifier:
          node.expression && ts.isStringLiteralLike(node.expression)
            ? node.expression.text
            : undefined,
      });
    } else if (
      ts.isImportTypeNode(node) &&
      ts.isLiteralTypeNode(node.argument) &&
      ts.isStringLiteralLike(node.argument.literal)
    ) {
      result.push({ node, specifier: node.argument.literal.text, typeOnly: true });
    }
    ts.forEachChild(node, visit);
  }
  visit(source);
  return result;
}

/** Follow named imports, namespace imports and local aliases using lexical symbols.
 * Shadowed globals stay local; no installed package code is loaded or executed.
 */
function referenceOrigin(
  checker: ts.TypeChecker,
  node: ts.Node,
  seen = new Set<ts.Node>(),
): string | undefined {
  if (seen.has(node)) return;
  seen.add(node);
  if (
    ts.isParenthesizedExpression(node) ||
    ts.isAsExpression(node) ||
    ts.isNonNullExpression(node) ||
    ts.isSatisfiesExpression(node)
  )
    return referenceOrigin(checker, node.expression, seen);
  if (ts.isPropertyAccessExpression(node) || ts.isElementAccessExpression(node)) {
    const member = ts.isPropertyAccessExpression(node)
      ? node.name.text
      : ts.isStringLiteralLike(node.argumentExpression)
        ? node.argumentExpression.text
        : undefined;
    const base = referenceOrigin(checker, node.expression, seen);
    return base && member ? `${base}.${member}` : undefined;
  }
  if (!ts.isIdentifier(node)) return;
  const parent = node.parent;
  if (
    (ts.isBindingElement(parent) && parent.propertyName === node) ||
    (ts.isPropertyAssignment(parent) && parent.name === node) ||
    (ts.isPropertyAccessExpression(parent) && parent.name === node)
  )
    return;
  const symbol = ts.isShorthandPropertyAssignment(parent)
    ? checker.getShorthandAssignmentValueSymbol(parent)
    : checker.getSymbolAtLocation(node);
  if (!symbol) return node.text;
  const declaration = symbol.valueDeclaration ?? symbol.declarations?.[0];
  if (!declaration) return node.text === "globalThis" ? "globalThis" : undefined;
  if (ts.isVariableDeclaration(declaration) && declaration.initializer)
    return referenceOrigin(checker, declaration.initializer, seen);
  if (ts.isBindingElement(declaration) && ts.isObjectBindingPattern(declaration.parent)) {
    const variable = declaration.parent.parent;
    if (ts.isVariableDeclaration(variable) && variable.initializer) {
      const base = referenceOrigin(checker, variable.initializer, seen);
      const key = declaration.propertyName ?? declaration.name;
      if (base && (ts.isIdentifier(key) || ts.isStringLiteralLike(key)))
        return `${base}.${key.text}`;
    }
  }
  if (
    ts.isImportSpecifier(declaration) ||
    ts.isNamespaceImport(declaration) ||
    ts.isImportClause(declaration)
  ) {
    let parent: ts.Node = declaration;
    while (parent && !ts.isImportDeclaration(parent)) parent = parent.parent;
    if (
      parent &&
      ts.isImportDeclaration(parent) &&
      ts.isStringLiteralLike(parent.moduleSpecifier)
    ) {
      const module = parent.moduleSpecifier.text.replace(/^node:/, "");
      return ts.isImportSpecifier(declaration)
        ? `${module}.${(declaration.propertyName ?? declaration.name).text}`
        : module;
    }
  }
}

/** Check repository architecture from source snapshots. Policies inspect local
 * dependency closures, including re-exports and literal dynamic imports. Package
 * internals and arbitrary computed JavaScript are outside this static check.
 */
export function checkArchitecture(sources: ReadonlyMap<string, string>): string[] {
  const diagnostics: string[] = [];
  for (const [path, source] of sources) diagnostics.push(...rustPolicy(path, source));
  const typescript = new Map([...sources].filter(([path]) => path.endsWith(".ts")));
  const options: ts.CompilerOptions = {
    noLib: true,
    noResolve: true,
    moduleDetection: ts.ModuleDetectionKind.Force,
    module: ts.ModuleKind.ESNext,
    target: ts.ScriptTarget.Latest,
  };
  const host = ts.createCompilerHost(options);
  // Preserve the program's module-detection callback when parsing. Pre-parsed
  // script files would otherwise share globals despite moduleDetection: Force.
  host.getSourceFile = (path, languageOptions) => {
    const source = typescript.get(path);
    return source === undefined
      ? undefined
      : ts.createSourceFile(path, source, languageOptions, true);
  };
  host.fileExists = (path) => typescript.has(path);
  host.readFile = (path) => typescript.get(path);
  host.getCurrentDirectory = () => "";
  const program = ts.createProgram([...typescript.keys()], options, host);
  const files = new Map(program.getSourceFiles().map((source) => [source.fileName, source]));
  const checker = program.getTypeChecker();
  const imports = new Map([...files].map(([path, source]) => [path, dependencies(source)]));
  const report = (path: string, node: ts.Node, rule: string, message: string) => {
    const { line, character } = node.getSourceFile().getLineAndCharacterOfPosition(node.getStart());
    const diagnostic = `${path}:${line + 1}:${character + 1} [${rule}] ${message}`;
    if (!diagnostics.includes(diagnostic)) diagnostics.push(diagnostic);
  };
  const production = new Set<string>();
  function include(path: string) {
    if (production.has(path) || !files.has(path)) return;
    production.add(path);
    for (const { node, specifier, typeOnly } of imports.get(path) ?? []) {
      if (!specifier)
        report(
          path,
          node,
          "command-entry",
          "Use literal module specifiers so production dependencies can be checked.",
        );
      else if (specifier.startsWith(".")) {
        const target = localModule(path, specifier);
        if (!specifier.endsWith(".ts"))
          report(
            path,
            node,
            "command-entry",
            "Import local TypeScript sources with an explicit .ts extension; no JavaScript or emitted-source route.",
          );
        else if (!files.has(target))
          report(
            path,
            node,
            "command-entry",
            `Cannot resolve local TypeScript dependency ${specifier}.`,
          );
        else if (!typeOnly) include(target);
      }
    }
  }
  for (const path of files.keys())
    if ((path.startsWith("tools/") || path.startsWith("bench/")) && !testSource(path))
      include(path);
  const inspected = { "plot-purity": new Set<string>(), "offline-report": new Set<string>() };

  function dependencyClosure(root: string, boundary: "plot-purity" | "offline-report") {
    const visited = inspected[boundary];
    function visit(path: string, chain: string[]) {
      if (visited.has(path)) return;
      visited.add(path);
      const source = files.get(path)!;
      for (const dependency of imports.get(path) ?? []) {
        const { node, specifier } = dependency;
        const explain = (message: string) =>
          report(path, node, boundary, `${message} (${chain.join(" -> ")})`);
        if (!specifier) {
          explain("Use literal module specifiers so the dependency boundary can be checked.");
        } else if (specifier.startsWith(".")) {
          const target = localModule(path, specifier);
          const allowed =
            target.startsWith("bench/plot/") ||
            (boundary === "offline-report" &&
              (target.startsWith("bench/report/") || reportModules.has(target)));
          if (!allowed) explain(`Dependency ${target} crosses the ${boundary} boundary.`);
          else if (!specifier.endsWith(".ts") || !files.has(target))
            explain(
              `Cannot resolve local TypeScript dependency ${specifier}; use an explicit .ts path.`,
            );
          else visit(target, [...chain, target]);
        } else if (
          !(boundary === "plot-purity" ? plotPackages : reportPackages).has(
            specifier.replace(/^node:/, ""),
          )
        ) {
          explain(`Dependency ${specifier} is outside the ${boundary} package contract.`);
        }
      }
      function inspect(node: ts.Node) {
        if (
          ts.isIdentifier(node) ||
          ts.isPropertyAccessExpression(node) ||
          ts.isElementAccessExpression(node)
        ) {
          const origin = referenceOrigin(checker, node)?.replace(/^globalThis\./, "");
          if (origin) {
            const global = origin.split(".")[0];
            const network = [
              "fetch",
              "WebSocket",
              "EventSource",
              "XMLHttpRequest",
              "Worker",
              "SharedWorker",
              "Bun",
              "Deno",
            ].includes(global);
            const impurePlot =
              [
                "document",
                "window",
                "navigator",
                "location",
                "localStorage",
                "sessionStorage",
                "process",
                "Date",
                "performance",
                "crypto",
              ].includes(global) || origin === "Math.random";
            if (network || (boundary === "plot-purity" && impurePlot))
              report(
                path,
                node,
                boundary,
                `Do not use ${origin}; pass prepared data into this operation. (${chain.join(" -> ")})`,
              );
          }
        }
        ts.forEachChild(node, inspect);
      }
      inspect(source);
    }
    visit(root, [root]);
  }

  for (const [path, source] of files) {
    if (!production.has(path)) continue;
    for (const dependency of imports.get(path) ?? []) {
      const module = dependency.specifier?.replace(/^node:/, "");
      if (
        path !== "tools/process.ts" &&
        !dependency.typeOnly &&
        ((module === "child_process" &&
          (!dependency.names || dependency.names.some((name) => !synchronousCommands.has(name)))) ||
          module === "bun" ||
          module === "module")
      )
        report(
          path,
          dependency.node,
          "command-entry",
          "Start asynchronous subprocesses through tools/process.ts; only explicit named synchronous child_process imports are exempt.",
        );
    }
    function inspect(node: ts.Node) {
      if (
        ts.isIdentifier(node) ||
        ts.isPropertyAccessExpression(node) ||
        ts.isElementAccessExpression(node)
      ) {
        const origin = referenceOrigin(checker, node)?.replace(/^globalThis\./, "");
        if (
          origin &&
          (/^Bun\.(spawn|spawnSync|\$)$/.test(origin) || origin === "process.getBuiltinModule")
        )
          report(
            path,
            node,
            "command-entry",
            "Start subprocesses through tools/process.ts so cancellation, diagnostics and errors share one owner.",
          );
      }
      ts.forEachChild(node, inspect);
    }
    if (path !== "tools/process.ts") inspect(source);
    if (path.startsWith("bench/plot/")) dependencyClosure(path, "plot-purity");
    if (path.startsWith("bench/report/")) dependencyClosure(path, "offline-report");
  }
  return diagnostics.sort();
}

export async function repositorySources(): Promise<Map<string, string>> {
  const paths = execFileSync(
    "git",
    [
      "ls-files",
      "-z",
      "--cached",
      "--others",
      "--exclude-standard",
      "--",
      "*.ts",
      "rust/tessera-order/src/*.rs",
    ],
    { encoding: "utf8" },
  )
    .split("\0")
    .filter((path) => /\.(ts|rs)$/.test(path));
  const sources = await Promise.all(
    [...new Set(paths)].map(async (path) => {
      try {
        return [path, await readFile(path, "utf8")] as const;
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code === "ENOENT") return null;
        throw error;
      }
    }),
  );
  return new Map(sources.filter((source) => source !== null));
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const diagnostics = checkArchitecture(await repositorySources());
  if (diagnostics.length) {
    console.error(diagnostics.join("\n"));
    process.exitCode = 1;
  } else console.log("Architecture policies passed.");
}
