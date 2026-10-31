"use strict";
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const root = path.resolve(__dirname, "../..");
const html = fs.readFileSync(path.join(root, "index.html"), "utf8");
const script = html.match(/<script>([\s\S]*?)<\/script>/i)?.[1];
assert.ok(script, "Root Pages entry must contain its fixed local redirect");
assert.ok(fs.existsSync(path.join(root, ".nojekyll")));
assert.ok(fs.existsSync(path.join(root, "website/dist/index.html")));
assert.match(html, /url=website\/dist\//);
assert.match(html, /href="website\/dist\/"/);
for (const [source, expected] of [
  [
    "https://example.github.io/Trinix/?from=home#download",
    "https://example.github.io/Trinix/website/dist/?from=home#download",
  ],
  [
    "https://example.github.io/?next=https://untrusted.example/#progress",
    "https://example.github.io/website/dist/?next=https://untrusted.example/#progress",
  ],
  [
    "file:///project/Trinix/index.html#download",
    "file:///project/Trinix/website/dist/#download",
  ],
]) {
  const input = new URL(source);
  let destination;
  const location = {
    href: source,
    search: input.search,
    hash: input.hash,
    replace(value) {
      destination = value;
    },
  };
  vm.runInNewContext(script, { location, URL });
  assert.equal(destination, expected);
}
console.log(
  "Passed: root Pages entry, repository subpath, custom domain, local file and fragment/query preservation.",
);
