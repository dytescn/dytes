// 创建包含 child 字段的新表（routers）
const createTableRes = await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: {
    "Content-Type": "application/libary",
    "FFI-Symbol": "db_create",
  },
  body: JSON.stringify({
    path: "./router",
    table: "routers",
    schema: `id INTEGER PRIMARY KEY,
             title TEXT,
             icon TEXT,
             hide INTEGER,
             path TEXT,
             child TEXT,
             url TEXT,
             show INTEGER,
             parent_id INTEGER,
             level INTEGER`,
  }),
});

console.log(await createTableRes.json());

// 清理旧的项目路由
await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: {
    "Content-Type": "application/libary",
    "FFI-Symbol": "db_insert",
  },
  body: JSON.stringify({
    path: "router",
    sql: "DELETE FROM routers WHERE parent_id IN (SELECT id FROM routers WHERE title = '项目')",
  }),
});

await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: {
    "Content-Type": "application/libary",
    "FFI-Symbol": "db_insert",
  },
  body: JSON.stringify({
    path: "router",
    sql: "DELETE FROM routers WHERE title = '项目'",
  }),
});

// 插入项目路由
await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: {
    "Content-Type": "application/libary",
    "FFI-Symbol": "db_insert",
  },
  body: JSON.stringify({
    path: "router",
    sql: `
        INSERT INTO routers (id, title, icon, hide, path, parent_id, level)
        VALUES (3, '项目', 'ic-project', 0, '/project', 0, 1)
    `,
  }),
});

await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: {
    "Content-Type": "application/libary",
    "FFI-Symbol": "db_insert",
  },
  body: JSON.stringify({
    path: "router",
    sql: `
        INSERT INTO routers (id, title, icon, hide, path, url, show, parent_id, level)
        VALUES (4, 'project', '', 0, '/project', '/project/project.js', 1, 1, 2)
    `,
  }),
});

console.log("项目路由插入成功");

// 创建项目数据库表
await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: {
    "Content-Type": "application/libary",
    "FFI-Symbol": "db_update",
  },
  body: JSON.stringify({
    path: "./project",
    sql: `
        CREATE TABLE IF NOT EXISTS project (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            uuid TEXT,
            organ_uid TEXT,
            create_by TEXT,
            pro_tpl_uid TEXT,
            cover TEXT,
            name TEXT,
            description TEXT,
            sort INTEGER,
            schedule TEXT,
            private INTEGER,
            archive INTEGER DEFAULT 2,
            archive_time TEXT,
            open_begin_time TEXT,
            open_task_private TEXT,
            begin_time TEXT,
            end_time TEXT,
            recycle_time TEXT,
            is_recycle INTEGER DEFAULT 2,
            auto_update_schedule INTEGER,
            created_at TEXT,
            updated_at TEXT,
            deleted_at TEXT
        )
    `,
  }),
});

console.log("项目数据库表创建成功");
console.log("所有项目初始化任务完成！");