#!/usr/bin/env node
// The A0 oracle: run Quietust's Visual 2A03 JavaScript engine headlessly
// and record the logic level of EVERY node after init and after every
// MASTER half-step (one clk_in toggle; the ÷12 clk0 is an output the
// divider produces). The Rust engine is diffed against this, bit for
// bit: the family's golden-trace pattern, on the fifth chip.
//
// Only the data files, wires.js and chipsim.js are loaded: the macro
// layer (bus handlers, tables, sound canvases) is display-and-scripting
// machinery, so init and stepping are implemented here, statement for
// statement from macros.js initChip() (the fetched copy is hashed in
// tools/fetch-netlist.sh; the recipe is quoted in comments). The CPU
// buses are left undriven on both sides, which makes the comparison
// well defined without a memory model.
//
// Usage: node gen.js [--steps N] [--out FILE]

const fs = require('fs');
const path = require('path');
const vm = require('vm');

const REF = path.resolve(__dirname, '../../extern/visual2a03');

function arg(name, dflt) {
  const i = process.argv.indexOf(name);
  return i === -1 ? dflt : process.argv[i + 1];
}
const steps = parseInt(arg('--steps', '600'), 10);
const outPath = path.resolve(arg('--out', path.join(__dirname, 'golden-2a03.txt')));

const sandbox = {
  console,
  window: {},
  document: {
    getElementById: () => null,
    createElement: () => ({ appendChild() {}, childNodes: [] }),
    createTextNode: () => ({}),
  },
  navigator: { appVersion: '', appName: 'node' },
  location: { search: '' },
  setTimeout,
};
sandbox.global = sandbox;
vm.createContext(sandbox);

function load(file) {
  vm.runInContext(fs.readFileSync(path.join(REF, file), 'utf8'), sandbox, { filename: file });
}
load('segdefs.js');
load('transdefs.js');
load('nodenames.js');
load('wires.js');
load('chipsim.js');

vm.runInContext(
  `
  setupNodes();
  setupTransistors();

  // macros.js initChip(), verbatim in effect (sound canvases, refresh
  // and trace machinery omitted; they touch no node):
  for (var nn in nodes) { nodes[nn].state = false; nodes[nn].float = true; }
  nodes[ngnd].state = false; nodes[ngnd].float = false;
  nodes[npwr].state = true;  nodes[npwr].float = false;
  // "Turn on all transistors connected to VCC, and turn off the rest"
  for (var tn in transistors) transistors[tn].on = (transistors[tn].gate == npwr);
  setLow('clk_in');
  for (var i = 0; i < 6; i++) { setHigh('clk_in'); setLow('clk_in'); }
  setLow('res');
  setLow('so');
  setHigh('irq'); setHigh('nmi');
  recalcNodeList(allNodes());
  for (var i = 0; i < 12 * 8; i++) { setHigh('clk_in'); setLow('clk_in'); }
  setHigh('res');

  maxNode = nodes.length;
  dump = function () {
    var s = '';
    for (var i = 0; i < maxNode; i++) {
      s += (nodes[i] !== undefined && isNodeHigh(i)) ? '1' : '0';
    }
    return s;
  };
  masterHalfStep = function () {
    if (isNodeHigh(nodenames['clk_in'])) setLow('clk_in'); else setHigh('clk_in');
  };
`,
  sandbox
);

const run = (expr) => vm.runInContext(expr, sandbox);
const maxNode = run("maxNode");
console.error("js transistor count:", run("Object.keys(transistors).length"), "nodes:", run("nodes.filter(function(n){return n!==undefined}).length"));
const lines = [`2a03 golden: nodes ${maxNode} steps ${steps}`];
lines.push(run('dump()')); // the post-init state
const t0 = Date.now();
for (let i = 0; i < steps; i++) {
  run('masterHalfStep()');
  lines.push(run('dump()'));
  if (i % 50 === 49) {
    process.stderr.write(`\r${i + 1}/${steps} half steps, ${((Date.now() - t0) / 1000).toFixed(0)}s`);
  }
}
process.stderr.write('\n');
fs.writeFileSync(outPath, lines.join('\n') + '\n');
console.log(`wrote ${outPath} (${lines.length - 1} states over ${maxNode} nodes)`);
