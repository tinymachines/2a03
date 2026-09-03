#!/usr/bin/env node
// The A3 oracle: the reference simulator with MEMORY, running the
// authored square-note program (program-a3.json), recording every node
// after init and after every CPU half-step (macros.js halfStep: spin
// clk_in until clk0 flips, then service the bus exactly as
// handleBusRead/handleBusWrite do). The Rust harness is diffed against
// this bit for bit, which covers the APU, the core AND the bus glue.
//
// Usage: node gen-a3.js [--out FILE]

const fs = require('fs');
const path = require('path');
const vm = require('vm');

const REF = path.resolve(__dirname, '../../extern/visual2a03');
const prog = JSON.parse(fs.readFileSync(path.join(__dirname, 'program-a3.json'), 'utf8'));

function arg(name, dflt) {
  const i = process.argv.indexOf(name);
  return i === -1 ? dflt : process.argv[i + 1];
}
const outPath = path.resolve(arg('--out', path.join(__dirname, 'golden-2a03-a3.txt')));

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

sandbox.PROG = prog;
vm.runInContext(
  `
  setupNodes();
  setupTransistors();

  // macros.js initChip(), verbatim in effect:
  for (var nn in nodes) { nodes[nn].state = false; nodes[nn].float = true; }
  nodes[ngnd].state = false; nodes[ngnd].float = false;
  nodes[npwr].state = true;  nodes[npwr].float = false;
  for (var tn in transistors) transistors[tn].on = (transistors[tn].gate == npwr);
  setLow('clk_in');
  for (var i = 0; i < 6; i++) { setHigh('clk_in'); setLow('clk_in'); }
  setLow('res');
  setLow('so');
  setHigh('irq'); setHigh('nmi');
  recalcNodeList(allNodes());
  for (var i = 0; i < 12 * 8; i++) { setHigh('clk_in'); setLow('clk_in'); }
  setHigh('res');

  // macros.js memory + bus, statement for statement.
  memory = [];
  for (var i = 0; i < PROG.bytes.length; i++) memory[PROG.load + i] = PROG.bytes[i];
  memory[0xfffc] = PROG.reset_vector & 0xff;
  memory[0xfffd] = PROG.reset_vector >> 8;
  mRead = function (a) { return memory[a] == undefined ? 0 : memory[a]; };
  mWrite = function (a, d) { memory[a] = d; };
  readBits = function (name, n) {
    var res = 0;
    for (var i = 0; i < n; i++) {
      var nn = nodenames[name + i];
      res += ((isNodeHigh(nn)) ? 1 : 0) << i;
    }
    return res;
  };
  writeDataBus = function (x) {
    var recalcs = [];
    for (var i = 0; i < 8; i++) {
      var nn = nodenames['db' + i];
      var n = nodes[nn];
      if ((x % 2) == 0) { n.pulldown = true; n.pullup = false; }
      else { n.pulldown = false; n.pullup = true; }
      recalcs.push(nn);
      x >>= 1;
    }
    recalcNodeList(recalcs);
  };
  handleBusRead = function () {
    if (isNodeHigh(nodenames['rw'])) writeDataBus(mRead(readBits('ab', 16)));
  };
  handleBusWrite = function () {
    if (!isNodeHigh(nodenames['rw'])) mWrite(readBits('ab', 16), readBits('db', 8));
  };
  halfStep = function () {
    var clk = isNodeHigh(nodenames['clk0']);
    do { setHigh('clk_in'); setLow('clk_in'); }
    while (clk == isNodeHigh(nodenames['clk0']));
    if (clk) { handleBusRead(); } else { handleBusWrite(); }
  };

  maxNode = nodes.length;
  dump = function () {
    var s = '';
    for (var i = 0; i < maxNode; i++) {
      s += (nodes[i] !== undefined && isNodeHigh(i)) ? '1' : '0';
    }
    return s;
  };
`,
  sandbox
);

const run = (expr) => vm.runInContext(expr, sandbox);
const maxNode = run("maxNode");
const steps = prog.half_steps;
const lines = [`2a03 a3 golden: nodes ${maxNode} steps ${steps}`];
lines.push(run('dump()'));
const t0 = Date.now();
for (let i = 0; i < steps; i++) {
  run('halfStep()');
  lines.push(run('dump()'));
  if (i % 20 === 19) {
    process.stderr.write(`\r${i + 1}/${steps} half steps, ${((Date.now() - t0) / 1000).toFixed(0)}s`);
  }
}
process.stderr.write('\n');
fs.writeFileSync(outPath, lines.join('\n') + '\n');
console.log(`wrote ${outPath} (${lines.length - 1} states over ${maxNode} nodes)`);
