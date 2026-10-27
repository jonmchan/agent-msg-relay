#!/usr/bin/env node
"use strict";

const { spawn } = require("node:child_process");
const { createRequire } = require("node:module");

const packages = {
  "darwin-arm64": ["agent-msg-relay-darwin-arm64", "agent-msg-relay"],
  "darwin-x64": ["agent-msg-relay-darwin-x64", "agent-msg-relay"],
  "linux-arm64": ["agent-msg-relay-linux-arm64", "agent-msg-relay"],
  "linux-x64": ["agent-msg-relay-linux-x64", "agent-msg-relay"],
  "win32-x64": ["agent-msg-relay-win32-x64", "agent-msg-relay.exe"],
};

const key = process.platform + "-" + process.arch;
const selected = packages[key];
if (!selected) {
  const supported = Object.keys(packages).join(", ");
  console.error(
    "agent-msg-relay has no binary for " +
      key +
      ". Supported: " +
      supported +
      "."
  );
  process.exit(1);
}

const pkg = selected[0];
const binary = selected[1];
let binaryPath;
try {
  binaryPath = createRequire(__filename).resolve(pkg + "/" + binary);
} catch {
  console.error(
    "agent-msg-relay could not find the " +
      pkg +
      " package. Install this release with npm. Do not download a binary."
  );
  process.exit(1);
}

const child = spawn(binaryPath, process.argv.slice(2), {
  stdio: "inherit",
  cwd: process.cwd(),
});

child.on("error", (err) => {
  console.error(err.message);
  process.exit(1);
});

child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code === null ? 1 : code);
});
