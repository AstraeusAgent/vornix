#!/usr/bin/env node
// sable-shell: MCP server for shell/process control
// Speaks JSON-RPC 2.0 over stdio

const { spawn, execSync } = require("child_process");
const readline = require("readline");

const WORKSPACE = process.env.SABLE_WORKSPACE || process.cwd();
const processes = new Map();
let nextProcId = 1;

function send(obj) {
  process.stdout.write(JSON.stringify(obj) + "\n");
}

function ok(id, result) {
  send({ jsonrpc: "2.0", id, result });
}

function err(id, code, message) {
  send({ jsonrpc: "2.0", id, error: { code, message } });
}

const TOOLS = [
  {
    name: "execute",
    description: "Execute a shell command and return stdout/stderr. Use for one-shot commands.",
    inputSchema: {
      type: "object",
      properties: {
        command: { type: "string", description: "Shell command to execute" },
        cwd: { type: "string", description: "Working directory (optional)" },
        timeout: { type: "number", description: "Timeout in ms (default 30000)" },
      },
      required: ["command"],
    },
  },
  {
    name: "start_process",
    description: "Start a long-running background process.",
    inputSchema: {
      type: "object",
      properties: {
        command: { type: "string" },
        args: { type: "array", items: { type: "string" } },
        cwd: { type: "string" },
      },
      required: ["command"],
    },
  },
  {
    name: "list_processes",
    description: "List running background processes.",
    inputSchema: { type: "object", properties: {} },
  },
  {
    name: "kill_process",
    description: "Kill a background process by ID.",
    inputSchema: {
      type: "object",
      properties: { id: { type: "number" } },
      required: ["id"],
    },
  },
  {
    name: "tail_output",
    description: "Get recent stdout/stderr from a background process.",
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "number" },
        lines: { type: "number" },
      },
      required: ["id"],
    },
  },
];

function executeCommand(args) {
  const cwd = args.cwd || WORKSPACE;
  const timeout = args.timeout || 30000;

  try {
    const output = execSync(args.command, {
      cwd,
      timeout,
      encoding: "utf-8",
      maxBuffer: 1024 * 1024 * 10, // 10MB
      shell: process.env.SHELL || "/bin/bash",
    });
    return { stdout: output.trim(), stderr: "", exitCode: 0 };
  } catch (e) {
    return {
      stdout: (e.stdout || "").toString().trim(),
      stderr: (e.stderr || "").toString().trim(),
      exitCode: e.status || 1,
    };
  }
}

function startProcess(args) {
  const cmd = args.command;
  const cmdArgs = args.args || [];
  const cwd = args.cwd || WORKSPACE;

  const child = spawn(cmd, cmdArgs, {
    cwd,
    shell: process.env.SHELL || "/bin/bash",
    stdio: ["ignore", "pipe", "pipe"],
  });

  const id = nextProcId++;
  const procInfo = {
    id,
    command: `${cmd} ${cmdArgs.join(" ")}`.trim(),
    pid: child.pid,
    stdout: [],
    stderr: [],
    running: true,
  };
  processes.set(id, procInfo);

  child.stdout.on("data", (data) => {
    procInfo.stdout.push(...data.toString().split("\n").filter(Boolean));
    if (procInfo.stdout.length > 1000) procInfo.stdout.splice(0, 500);
  });
  child.stderr.on("data", (data) => {
    procInfo.stderr.push(...data.toString().split("\n").filter(Boolean));
    if (procInfo.stderr.length > 1000) procInfo.stderr.splice(0, 500);
  });
  child.on("close", (code) => {
    procInfo.running = false;
    procInfo.exitCode = code;
  });

  return { id, pid: child.pid, command: procInfo.command };
}

function listProcesses() {
  const procs = [];
  for (const [id, info] of processes.entries()) {
    procs.push({ id, pid: info.pid, command: info.command, running: info.running });
  }
  return { processes: procs };
}

function killProcess(args) {
  const proc = processes.get(args.id);
  if (!proc) return { error: "Process not found" };
  try {
    process.kill(proc.pid, "SIGTERM");
    proc.running = false;
    return { killed: true };
  } catch (e) {
    return { error: e.message };
  }
}

function tailOutput(args) {
  const proc = processes.get(args.id);
  if (!proc) return { error: "Process not found" };
  const n = args.lines || 50;
  return {
    stdout: proc.stdout.slice(-n).join("\n"),
    stderr: proc.stderr.slice(-n).join("\n"),
    running: proc.running,
  };
}

function handleRequest(req) {
  const { id, method, params } = req;

  if (method === "initialize") {
    return ok(id, {
      protocolVersion: "2025-03-26",
      capabilities: { tools: { listChanged: false } },
      serverInfo: { name: "sable-shell", version: "0.1.0" },
    });
  }

  if (method === "notifications/initialized") return;

  if (method === "tools/list") {
    return ok(id, { tools: TOOLS });
  }

  if (method === "tools/call") {
    const { name, arguments: args } = params;
    try {
      let result;
      switch (name) {
        case "execute": result = executeCommand(args); break;
        case "start_process": result = startProcess(args); break;
        case "list_processes": result = listProcesses(); break;
        case "kill_process": result = killProcess(args); break;
        case "tail_output": result = tailOutput(args); break;
        default:
          return err(id, -32601, `Unknown tool: ${name}`);
      }
      return ok(id, {
        content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        isError: false,
      });
    } catch (e) {
      return ok(id, {
        content: [{ type: "text", text: e.message }],
        isError: true,
      });
    }
  }

  err(id, -32601, `Unknown method: ${method}`);
}

const rl = readline.createInterface({ input: process.stdin, terminal: false });
rl.on("line", (line) => {
  try {
    const req = JSON.parse(line.trim());
    handleRequest(req);
  } catch (e) {
    process.stderr.write(`sable-shell: parse error: ${e.message}\n`);
  }
});
rl.on("close", () => process.exit(0));

process.stderr.write("sable-shell: started\n");