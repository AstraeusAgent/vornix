#!/usr/bin/env node
// sable-fs: MCP server for filesystem operations
// Speaks JSON-RPC 2.0 over stdio

const fs = require("fs");
const path = require("path");
const { globSync } = require("glob");
const readline = require("readline");

const WORKSPACE = process.env.SABLE_WORKSPACE || process.cwd();

function send(obj) {
  process.stdout.write(JSON.stringify(obj) + "\n");
}

function ok(id, result) {
  send({ jsonrpc: "2.0", id, result });
}

function err(id, code, message) {
  send({ jsonrpc: "2.0", id, error: { code, message } });
}

function notify(method, params) {
  send({ jsonrpc: "2.0", method, params });
}

const TOOLS = [
  {
    name: "read_file",
    description: "Read a file's contents. Supports optional line range.",
    inputSchema: {
      type: "object",
      properties: {
        path: { type: "string", description: "File path (relative to workspace or absolute)" },
        startLine: { type: "number", description: "Start line (1-indexed, optional)" },
        endLine: { type: "number", description: "End line (inclusive, optional)" },
      },
      required: ["path"],
    },
  },
  {
    name: "write_file",
    description: "Write content to a file, creating directories as needed.",
    inputSchema: {
      type: "object",
      properties: {
        path: { type: "string" },
        content: { type: "string" },
      },
      required: ["path", "content"],
    },
  },
  {
    name: "list_directory",
    description: "List files in a directory, respecting .gitignore.",
    inputSchema: {
      type: "object",
      properties: {
        path: { type: "string" },
        maxDepth: { type: "number" },
      },
    },
  },
  {
    name: "glob_search",
    description: "Search for files matching a glob pattern.",
    inputSchema: {
      type: "object",
      properties: {
        pattern: { type: "string" },
        cwd: { type: "string" },
      },
      required: ["pattern"],
    },
  },
  {
    name: "file_info",
    description: "Get file metadata (size, mtime, type).",
    inputSchema: {
      type: "object",
      properties: { path: { type: "string" } },
      required: ["path"],
    },
  },
];

function resolvePath(p) {
  if (path.isAbsolute(p)) return p;
  return path.join(WORKSPACE, p);
}

function readFile(args) {
  const fp = resolvePath(args.path);
  let content = fs.readFileSync(fp, "utf-8");
  if (args.startLine || args.endLine) {
    const lines = content.split("\n");
    const start = Math.max(0, (args.startLine || 1) - 1);
    const end = args.endLine ? Math.min(lines.length, args.endLine) : lines.length;
    content = lines.slice(start, end).join("\n");
  }
  return { content, path: fp };
}

function writeFile(args) {
  const fp = resolvePath(args.path);
  fs.mkdirSync(path.dirname(fp), { recursive: true });
  fs.writeFileSync(fp, args.content, "utf-8");
  return { path: fp, bytes: Buffer.byteLength(args.content, "utf-8") };
}

function listDir(args) {
  const fp = resolvePath(args.path || ".");
  const maxDepth = args.maxDepth || 2;
  const entries = [];
  
  function walk(dir, depth) {
    if (depth > maxDepth) return;
    let items;
    try {
      items = fs.readdirSync(dir, { withFileTypes: true });
    } catch { return; }
    for (const item of items) {
      if (item.name === ".git" || item.name === "node_modules") continue;
      const fullPath = path.join(dir, item.name);
      const rel = path.relative(fp, fullPath);
      entries.push({ name: rel, type: item.isDirectory() ? "directory" : "file" });
      if (item.isDirectory()) walk(fullPath, depth + 1);
    }
  }
  walk(fp, 0);
  return { entries };
}

function globSearch(args) {
  const cwd = args.cwd ? resolvePath(args.cwd) : WORKSPACE;
  const matches = globSync(args.pattern, { cwd, nodir: true, ignore: ["**/node_modules/**", "**/.git/**"] });
  return { matches };
}

function fileInfo(args) {
  const fp = resolvePath(args.path);
  const stat = fs.statSync(fp);
  return {
    path: fp,
    size: stat.size,
    mtime: stat.mtime.toISOString(),
    isDirectory: stat.isDirectory(),
    isFile: stat.isFile(),
    mode: stat.mode.toString(8),
  };
}

function handleRequest(req) {
  const { id, method, params } = req;

  if (method === "initialize") {
    return ok(id, {
      protocolVersion: "2025-03-26",
      capabilities: { tools: { listChanged: false } },
      serverInfo: { name: "sable-fs", version: "0.1.0" },
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
        case "read_file": result = readFile(args); break;
        case "write_file": result = writeFile(args); break;
        case "list_directory": result = listDir(args); break;
        case "glob_search": result = globSearch(args); break;
        case "file_info": result = fileInfo(args); break;
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
    process.stderr.write(`sable-fs: parse error: ${e.message}\n`);
  }
});
rl.on("close", () => process.exit(0));

process.stderr.write("sable-fs: started\n");