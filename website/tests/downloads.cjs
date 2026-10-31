const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync(require('node:path').join(__dirname, '../dist/app.js'), 'utf8');
class Element {
  constructor() { this.children = []; this.textContent = ''; this.disabled = false; }
  replaceChildren(...children) { this.children = children; }
  append(child) { this.children.push(child); }
  setAttribute(key, value) { this[key] = value; }
  addEventListener() {}
}
async function render(response, failure = false) {
  const status = new Element(), refresh = new Element();
  const cards = Object.fromEntries(['windows', 'linux', 'android'].map(platform => {
    const links = new Element();
    return [platform, { links, querySelector: () => links }];
  }));
  const document = {
    querySelectorAll(selector) { return selector === '[data-platform]' ? Object.values(cards) : []; },
    getElementById(id) { return id === 'release-status' ? status : refresh; },
    createElement() { return new Element(); },
    createTextNode(text) { return { textContent: text }; },
    querySelector(selector) { return cards[selector.match(/data-platform="(\w+)"/)[1]].links; }
  };
  const context = vm.createContext({ document, URL, AbortController, setTimeout, clearTimeout,
    fetch: async () => { if (failure) throw new Error('offline'); return response; }
  });
  vm.runInContext(source, context);
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(refresh.disabled, false);
  return { cards, status, context };
}
(async () => {
  let result = await render({ status: 404 });
  assert.match(result.status.textContent, /No published release/);
  for (const card of Object.values(result.cards)) assert.equal(card.links.children[0].disabled, true);
  const asset = (name, url) => ({ name, browser_download_url: url || `https://github.com/reduckyLang/Trinix/releases/download/v1/${name}` });
  result = await render({ status: 200, ok: true, json: async () => ({ tag_name: 'v1', assets: [
    asset('Trinix.msi'), asset('Trinix-Setup-x64.exe'), asset('Trinix.deb'),
    asset('Trinix.rpm'), asset('Trinix.apk'), asset('mm-viewer.exe'), asset('source.zip'),
    asset('malicious.msi', 'https://example.org/untrusted.msi'),
    asset('other.msi', 'https://github.com/another/repository/releases/download/v1/other.msi')
  ] }) });
  assert.equal(result.cards.windows.links.children.length, 2);
  assert.equal(result.cards.linux.links.children.length, 2);
  assert.equal(result.cards.android.links.children.length, 1);
  assert.match(result.status.textContent, /5 installer packages/);
  result = await render({ status: 200, ok: true, json: async () => ({ tag_name: 'v2', assets: [asset('source.zip')] }) });
  assert.match(result.status.textContent, /No installer packages/);
  result = await render(null, true);
  assert.equal(result.status.children[1].href, 'https://github.com/reduckyLang/Trinix/releases');
  for (const card of Object.values(result.cards)) assert.equal(card.links.children[0].disabled, true);
  result = await render({ status: 403, ok: false });
  assert.equal(result.status.children[1].href, 'https://github.com/reduckyLang/Trinix/releases');
  console.log('Passed: no release, installer filtering, source-only release, offline and rate-limit fallback.');
})().catch(error => { console.error(error); process.exitCode = 1; });
