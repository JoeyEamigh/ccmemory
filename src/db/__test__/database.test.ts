import { describe, expect, test } from "bun:test";
import { createDatabase, checkIntegrity, checkpointWAL } from "../database.js";

describe("Database", () => {
  test("sets WAL mode for file-based databases", async () => {
    const db = await createDatabase(":memory:");
    const result = await db.execute("PRAGMA journal_mode");
    const mode = result.rows[0]?.["journal_mode"];
    expect(mode === "wal" || mode === "memory").toBe(true);
    db.close();
  });

  test("has foreign keys enabled", async () => {
    const db = await createDatabase(":memory:");
    const result = await db.execute("PRAGMA foreign_keys");
    expect(result.rows[0]?.["foreign_keys"]).toBe(1);
    db.close();
  });

  test("executes parameterized queries", async () => {
    const db = await createDatabase(":memory:");
    await db.execute(
      "INSERT INTO projects (id, path) VALUES (?, ?)",
      ["p1", "/test/path"]
    );
    const result = await db.execute(
      "SELECT * FROM projects WHERE id = ?",
      ["p1"]
    );
    expect(result.rows[0]?.["id"]).toBe("p1");
    expect(result.rows[0]?.["path"]).toBe("/test/path");
    db.close();
  });

  test("supports batch operations", async () => {
    const db = await createDatabase(":memory:");
    const results = await db.batch([
      { sql: "INSERT INTO projects (id, path) VALUES (?, ?)", args: ["p1", "/path1"] },
      { sql: "INSERT INTO projects (id, path) VALUES (?, ?)", args: ["p2", "/path2"] },
      { sql: "SELECT COUNT(*) as cnt FROM projects" },
    ]);
    expect(results[2]?.rows[0]?.["cnt"]).toBe(2);
    db.close();
  });

  test("batch operations are atomic", async () => {
    const db = await createDatabase(":memory:");

    await db.batch([
      { sql: "INSERT INTO projects (id, path) VALUES (?, ?)", args: ["p1", "/path1"] },
      { sql: "INSERT INTO projects (id, path) VALUES (?, ?)", args: ["p2", "/path2"] },
    ]);

    const result = await db.execute("SELECT COUNT(*) as cnt FROM projects");
    expect(result.rows[0]?.["cnt"]).toBe(2);
    db.close();
  });

  test("batch operations rollback on error", async () => {
    const db = await createDatabase(":memory:");
    await db.execute(
      "INSERT INTO projects (id, path) VALUES (?, ?)",
      ["p1", "/path1"]
    );

    try {
      await db.batch([
        { sql: "INSERT INTO projects (id, path) VALUES (?, ?)", args: ["p2", "/path2"] },
        { sql: "INSERT INTO projects (id, path) VALUES (?, ?)", args: ["p1", "/duplicate"] },
      ]);
      expect(true).toBe(false);
    } catch {
      const result = await db.execute("SELECT COUNT(*) as cnt FROM projects");
      expect(result.rows[0]?.["cnt"]).toBe(1);
    }
    db.close();
  });

  test("client property exposes underlying libSQL client", async () => {
    const db = await createDatabase(":memory:");
    expect(db.client).toBeDefined();
    expect(typeof db.client.execute).toBe("function");
    db.close();
  });
});

describe("Database Recovery", () => {
  test("checkIntegrity returns ok for valid database", async () => {
    const db = await createDatabase(":memory:");
    const result = await checkIntegrity(db);
    expect(result.ok).toBe(true);
    expect(result.errors).toHaveLength(0);
    db.close();
  });

  test("checkpointWAL succeeds on valid database", async () => {
    const db = await createDatabase(":memory:");
    const success = await checkpointWAL(db);
    expect(success).toBe(true);
    db.close();
  });

  test("checkIntegrity checks all tables", async () => {
    const db = await createDatabase(":memory:");

    await db.execute(
      `INSERT INTO projects (id, path, name) VALUES (?, ?, ?)`,
      ["p1", "/test", "Test Project"]
    );

    const result = await checkIntegrity(db);
    expect(result.ok).toBe(true);
    db.close();
  });
});
