/**
 * Guards the one failure mode that will actually recur.
 *
 * Every colour in this app resolves through a CSS variable, which is what lets
 * one attribute flip the whole interface between light and dark. A literal like
 * `bg-white/6` is baked in at build time and cannot follow a variable, so it
 * survives the flip and quietly ruins exactly one scheme — the one nobody was
 * looking at. Ten of them accumulated before anyone noticed.
 *
 * Raw hex is checked too, because `text-[#8b8f96]` fails the same way.
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

const ROOT = "src";
const RULES = [
  [/\bbg-white\b|\bbg-black\b/, "hardcoded bg-white/bg-black — use var(--color-fill-subtle) or another token"],
  [/\b(?:bg|text|border)-\[#[0-9a-fA-F]{3,8}\]/, "raw hex in a utility — add a token instead"],
];

function* files(dir) {
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) yield* files(path);
    else if (/\.tsx?$/.test(path)) yield path;
  }
}

const failures = [];
for (const path of files(ROOT)) {
  readFileSync(path, "utf8").split(/\r?\n/).forEach((line, i) => {
    for (const [pattern, why] of RULES) {
      if (pattern.test(line)) failures.push(`${path}:${i + 1}  ${why}\n    ${line.trim()}`);
    }
  });
}

if (failures.length) {
  console.error(`Colour tokens: ${failures.length} problem(s)\n\n${failures.join("\n")}`);
  process.exit(1);
}
console.log("Colour tokens: clean.");
