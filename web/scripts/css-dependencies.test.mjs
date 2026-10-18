import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import test from 'node:test';

const require = createRequire(import.meta.url);
const postcss = require('postcss');
const nested = require('postcss-nested');

// Exercise the consumer of the scoped security override, not just the parser API.
async function rulesFor(css) {
  const result = await postcss([nested()]).process(css, { from: undefined });
  const rules = [];
  result.root.walkRules((rule) => {
    rules.push({
      selectors: rule.selectors,
      declarations: rule.nodes
        .filter((node) => node.type === 'decl')
        .map(({ prop, value }) => [prop, value]),
      parent: rule.parent.type === 'atrule' ? rule.parent.name : null,
    });
  });
  return rules;
}

test('nested code-block selectors preserve expansion and escaped classes', async () => {
  assert.deepEqual(await rulesFor(`
    .expressive-code, .code-frame {
      &.has-title { color: red; }
      & > pre:hover, & > pre:focus { color: blue; }
      .token\\:keyword { font-weight: bold; }
    }
  `), [
    { selectors: ['.expressive-code.has-title', '.code-frame.has-title'], declarations: [['color', 'red']], parent: null },
    { selectors: ['.expressive-code > pre:hover', '.expressive-code > pre:focus', '.code-frame > pre:hover', '.code-frame > pre:focus'], declarations: [['color', 'blue']], parent: null },
    { selectors: ['.expressive-code .token\\:keyword', '.code-frame .token\\:keyword'], declarations: [['font-weight', 'bold']], parent: null },
  ]);
});

test('functional selectors and nested media rules preserve parent substitution', async () => {
  assert.deepEqual(await rulesFor(`
    .expressive-code {
      &:is(:hover, :focus-within) { color: blue; }
      @media (min-width: 50rem) {
        &[data-language="js"] > pre { padding: 1rem; }
      }
    }
  `), [
    { selectors: ['.expressive-code:is(:hover, :focus-within)'], declarations: [['color', 'blue']], parent: null },
    { selectors: ['.expressive-code[data-language="js"] > pre'], declarations: [['padding', '1rem']], parent: 'media' },
  ]);
});

test('the nested plugin resolves the patched parser for flat selectors', () => {
  const nestedRequire = createRequire(require.resolve('postcss-nested'));
  assert.equal(nestedRequire('postcss-selector-parser/package.json').version, '7.1.6');
  const selector = '.a#b'.repeat(10_000);
  assert.equal(nestedRequire('postcss-selector-parser')().processSync(selector), selector);
});
