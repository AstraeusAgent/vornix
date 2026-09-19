#!/usr/bin/env node
// sable-thinking: MCP server for sequential thinking scratchpad
// Speaks JSON-RPC 2.0 over stdio

const readline = require("readline");

let thoughts = [];

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
    name: "think",
    description: "Externalize a chain of intermediate reasoning steps. Use for structured problem-solving, hypothesis tracking, and debugging analysis.",
    inputSchema: {
      type: "object",
      properties: {
        thought: { type: "string", description: "The reasoning step" },
        thoughtNumber: { type: "number", description: "Current step number" },
        totalThoughtsEstimate: { type: "number", description: "Estimated total steps" },
        isRevision: { type: "boolean", description: "Is this revising a prior thought?" },
        revisesThought: { type: "number", description: "Which thought number is being revised" },
      },
      required: ["thought", "thoughtNumber"],
    },
  },
  {
    name: "get_thoughts",
    description: "Retrieve all thoughts in the current chain.",
    inputSchema: {
      type: "object",
      properties: {},
    },
  },
  {
    name: "clear_thoughts",
    description: "Clear the thinking scratchpad.",
    inputSchema: {
      type: "object",
      properties: {},
    },
  },
];

function think(args) {
  const entry = {
    thoughtNumber: args.thoughtNumber,
    thought: args.thought,
    totalThoughtsEstimate: args.totalThoughtsEstimate || null,
    isRevision: args.isRevision || false,
    revisesThought: args.revisesThought || null,
    timestamp: new Date().toISOString(),
  };

  if (entry.isRevision && entry.revisesThought) {
    const idx = thoughts.findIndex((t) => t.thoughtNumber === entry.revisesThought);
    if (idx >= 0) {
      thoughts[idx] = entry;
    } else {
      thoughts.push(entry);
    }
  } else {
    thoughts.push(entry);
  }

  return {
    thoughtNumber: entry.thoughtNumber,
    totalStored: thoughts.length,
    acknowledged: true,
  };
}

function getThoughts() {
  return { thoughts: thoughts.slice() };
}

function clearThoughts() {
  const count = thoughts.length;
  thoughts = [];
  return { cleared: count };
}

function handleRequest(req) {
  const { id, method, params } = req;

  if (method === "initialize") {
    return ok(id, {
      protocolVersion: "2025-03-26",
      capabilities: { tools: { listChanged: false } },
      serverInfo: { name: "sable-thinking", version: "0.1.0" },
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
        case "think": result = think(args); break;
        case "get_thoughts": result = getThoughts(); break;
        case "clear_thoughts": result = clearThoughts(); break;
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
    process.stderr.write(`sable-thinking: parse error: ${e.message}\n`);
  }
});
rl.on("close", () => process.exit(0));

process.stderr.write("sable-thinking: started\n");